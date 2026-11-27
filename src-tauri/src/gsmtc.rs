//! Windows System Media Transport Controls (GSMTC) reader.
//!
//! Talks to whatever app Windows currently treats as the active media session
//! (Spotify, browsers, VLC, ...). Must be used from a single thread.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProps,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Storage::Streams::DataReader,
    Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
};

use crate::model::{Cmd, MediaState};

/// Stop re-trying the cover art fetch after this many polls for one track.
const MAX_ART_TRIES: u32 = 25;

pub struct Reader {
    manager: Option<SessionManager>,
    track_key: String,
    art: Option<String>,
    art_tries: u32,
}

impl Reader {
    pub fn new() -> Self {
        // WinRT needs the calling thread initialised. Failure here usually means
        // the thread already is (different apartment mode), which is fine.
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        Self {
            manager: None,
            track_key: String::new(),
            art: None,
            art_tries: 0,
        }
    }

    fn session(&mut self) -> Option<Session> {
        if self.manager.is_none() {
            self.manager = SessionManager::RequestAsync().and_then(|op| op.get()).ok();
        }
        self.manager.as_ref()?.GetCurrentSession().ok()
    }

    /// Fire a playback command at the current session. Errors are ignored:
    /// not every app supports every command.
    pub fn apply(&mut self, cmd: Cmd) {
        let Some(session) = self.session() else {
            return;
        };
        let _ = match cmd {
            Cmd::Toggle => session.TryTogglePlayPauseAsync().and_then(|op| op.get()),
            Cmd::Next => session.TrySkipNextAsync().and_then(|op| op.get()),
            Cmd::Prev => session.TrySkipPreviousAsync().and_then(|op| op.get()),
            Cmd::Seek(ms) => session
                .TryChangePlaybackPositionAsync(ms as i64 * 10_000)
                .and_then(|op| op.get()),
        };
    }

    /// Read the current state of the active media session.
    pub fn read(&mut self) -> MediaState {
        let Some(session) = self.session() else {
            self.forget_track();
            return MediaState::default();
        };
        let props = match session
            .TryGetMediaPropertiesAsync()
            .and_then(|op| op.get())
        {
            Ok(p) => p,
            Err(_) => return MediaState::default(),
        };

        let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
        let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
        let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
        let app = session
            .SourceAppUserModelId()
            .map(|s| s.to_string())
            .unwrap_or_default();

        // New track => drop the old cover. Some apps publish the thumbnail a
        // moment after the title, so keep trying for a while.
        let key = format!("{app}\u{1}{title}\u{1}{artist}\u{1}{album}");
        if key != self.track_key {
            self.track_key = key;
            self.art = None;
            self.art_tries = 0;
        }
        if self.art.is_none() && self.art_tries < MAX_ART_TRIES {
            self.art_tries += 1;
            self.art = read_thumbnail(&props);
        }

        let playing = session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .map(|s| s == Status::Playing)
            .unwrap_or(false);

        let (position_ms, duration_ms) = timeline(&session, playing);

        MediaState {
            active: !title.is_empty() || !artist.is_empty(),
            title,
            artist,
            album,
            app,
            playing,
            position_ms,
            duration_ms,
            art: self.art.clone(),
        }
    }

    fn forget_track(&mut self) {
        self.track_key.clear();
        self.art = None;
        self.art_tries = 0;
    }
}

/// Position and duration in milliseconds. While playing, the reported position
/// is extrapolated from `LastUpdatedTime`, since apps only refresh it sporadically.
fn timeline(session: &Session, playing: bool) -> (u64, u64) {
    let Ok(t) = session.GetTimelineProperties() else {
        return (0, 0);
    };
    let start = t.StartTime().map(|d| d.Duration).unwrap_or(0);
    let end = t.EndTime().map(|d| d.Duration).unwrap_or(0);
    let pos = t.Position().map(|d| d.Duration).unwrap_or(0);
    let updated = t.LastUpdatedTime().map(|d| d.UniversalTime).unwrap_or(0);

    let duration = (end - start).max(0);
    let mut position = (pos - start).max(0);
    if playing && updated > 0 {
        position += (now_ticks() - updated).max(0);
    }
    if duration > 0 {
        position = position.min(duration);
    }
    ((position / 10_000) as u64, (duration / 10_000) as u64)
}

/// Current time in Windows 100 ns ticks since 1601-01-01 (same epoch as `DateTime`).
fn now_ticks() -> i64 {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_secs() as i64 + 11_644_473_600) * 10_000_000 + (d.subsec_nanos() / 100) as i64
}

/// Cover art as a `data:` URL, or None if the app doesn't provide any.
fn read_thumbnail(props: &MediaProps) -> Option<String> {
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
    let size = u32::try_from(stream.Size().ok()?).ok()?;
    if size == 0 || size > 8 * 1024 * 1024 {
        return None;
    }
    let mime = stream
        .ContentType()
        .map(|s| s.to_string())
        .ok()
        .filter(|s| s.starts_with("image/"))
        .unwrap_or_else(|| "image/png".to_string());

    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size).ok()?.get().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(format!("data:{mime};base64,{}", STANDARD.encode(buf)))
}
