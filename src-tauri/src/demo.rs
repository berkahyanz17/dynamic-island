//! Stand-in media source for non-Windows builds, so the UI can be developed
//! anywhere. On Windows the real reader in `gsmtc.rs` is used instead.

use std::time::Instant;

use crate::model::{Cmd, MediaState};

const DURATION_MS: u64 = 214_000;

pub struct Reader {
    playing: bool,
    base_ms: u64,
    since: Instant,
}

impl Reader {
    pub fn new() -> Self {
        Self {
            playing: true,
            base_ms: 0,
            since: Instant::now(),
        }
    }

    fn position(&self) -> u64 {
        let extra = if self.playing {
            self.since.elapsed().as_millis() as u64
        } else {
            0
        };
        (self.base_ms + extra) % DURATION_MS
    }

    pub fn apply(&mut self, cmd: Cmd) {
        let now = self.position();
        match cmd {
            Cmd::Toggle => self.playing = !self.playing,
            Cmd::Seek(ms) => {
                self.base_ms = ms.min(DURATION_MS);
                self.since = Instant::now();
                return;
            }
            Cmd::Next | Cmd::Prev => {
                self.base_ms = 0;
                self.since = Instant::now();
                return;
            }
        }
        self.base_ms = now;
        self.since = Instant::now();
    }

    pub fn read(&mut self) -> MediaState {
        MediaState {
            active: true,
            title: "Demo Track".into(),
            artist: "Island".into(),
            album: "Preview".into(),
            app: "demo".into(),
            playing: self.playing,
            position_ms: self.position(),
            duration_ms: DURATION_MS,
            art: None,
        }
    }
}
