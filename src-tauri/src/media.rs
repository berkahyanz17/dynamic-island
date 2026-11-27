//! Media thread: polls the platform reader, emits state to the UI only when
//! something meaningful changed, and executes playback commands from the UI.

use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::model::{Cmd, MediaState};
use crate::vlc::{Config as VlcConfig, Vlc};

#[cfg(not(windows))]
use crate::demo::Reader;
#[cfg(windows)]
use crate::gsmtc::Reader;

const POLL: Duration = Duration::from_millis(400);
/// Re-emit when the real position differs from the extrapolated one by more than this (seeks).
const SEEK_THRESHOLD_MS: u64 = 1500;

pub struct MediaHandle {
    tx: Mutex<mpsc::Sender<Cmd>>,
    state: Mutex<MediaState>,
}

impl MediaHandle {
    fn send(&self, cmd: Cmd) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(cmd);
        }
    }
}

/// Register the managed handle and start the media thread.
pub fn start(app: &AppHandle) {
    let (tx, rx) = mpsc::channel();
    app.manage(MediaHandle {
        tx: Mutex::new(tx),
        state: Mutex::new(MediaState::default()),
    });

    let app = app.clone();
    std::thread::Builder::new()
        .name("media".into())
        .spawn(move || run(app, rx))
        .expect("failed to spawn media thread");
}

/// Which source the current state came from, so commands go to the right app.
#[derive(Clone, Copy)]
enum Source {
    System,
    Vlc,
}

/// The Windows media session plus VLC (which doesn't publish to it).
struct Sources {
    system: Reader,
    vlc: Vlc,
    current: Source,
}

impl Sources {
    fn new() -> Self {
        Self {
            system: Reader::new(),
            vlc: Vlc::new(VlcConfig::load()),
            current: Source::System,
        }
    }

    /// The system session wins ties; VLC takes over when it is playing and the
    /// system session isn't, or when the system has nothing at all.
    fn read(&mut self) -> MediaState {
        let system = self.system.read();
        match self.vlc.read() {
            Some(v) if (v.playing && !system.playing) || !system.active => {
                self.current = Source::Vlc;
                v
            }
            _ => {
                self.current = Source::System;
                system
            }
        }
    }

    fn apply(&mut self, cmd: Cmd) {
        match self.current {
            Source::System => self.system.apply(cmd),
            Source::Vlc => self.vlc.apply(cmd),
        }
    }
}

fn run(app: AppHandle, rx: mpsc::Receiver<Cmd>) {
    // The system reader owns WinRT objects, so this is created and used on this thread only.
    let mut reader = Sources::new();
    let mut last = MediaState::default();
    let mut last_emit = Instant::now();

    loop {
        match rx.recv_timeout(POLL) {
            Ok(cmd) => reader.apply(cmd),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        let next = reader.read();
        if !changed(&last, &next, last_emit) {
            continue;
        }
        last = next.clone();
        last_emit = Instant::now();

        let handle = app.state::<MediaHandle>();
        if let Ok(mut slot) = handle.state.lock() {
            *slot = next.clone();
        }
        let _ = app.emit("media://state", next);
    }
}

/// Everything except the position counts as a change; the position only counts
/// when it jumped (seek) relative to where steady playback would have put it.
fn changed(prev: &MediaState, next: &MediaState, prev_emitted_at: Instant) -> bool {
    if !prev.same_except_position(next) {
        return true;
    }
    let expected = prev.position_ms
        + if prev.playing {
            prev_emitted_at.elapsed().as_millis() as u64
        } else {
            0
        };
    next.position_ms.abs_diff(expected) > SEEK_THRESHOLD_MS
}

#[tauri::command]
pub fn get_state(handle: State<'_, MediaHandle>) -> MediaState {
    handle
        .state
        .lock()
        .map(|s| s.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn media_toggle(handle: State<'_, MediaHandle>) {
    handle.send(Cmd::Toggle);
}

#[tauri::command]
pub fn media_next(handle: State<'_, MediaHandle>) {
    handle.send(Cmd::Next);
}

#[tauri::command]
pub fn media_prev(handle: State<'_, MediaHandle>) {
    handle.send(Cmd::Prev);
}

#[tauri::command]
pub fn media_seek(handle: State<'_, MediaHandle>, position_ms: u64) {
    handle.send(Cmd::Seek(position_ms));
}
