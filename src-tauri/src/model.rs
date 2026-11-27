use serde::Serialize;

/// Snapshot of whatever Windows considers the current media session.
/// Serialised to the frontend as camelCase JSON.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaState {
    /// False when nothing is playing or paused anywhere.
    pub active: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// AppUserModelId of the source app, e.g. "Spotify.exe" or "chrome.exe".
    pub app: String,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// Cover art as a `data:` URL, if the source app provides one.
    pub art: Option<String>,
}

impl MediaState {
    /// Equal in every field except the playback position. Compares in place:
    /// cloning the state would copy the (possibly large) cover art every poll.
    pub fn same_except_position(&self, other: &Self) -> bool {
        // Exhaustive destructuring: adding a field forces a decision here.
        let Self {
            active,
            title,
            artist,
            album,
            app,
            playing,
            position_ms: _,
            duration_ms,
            art,
        } = self;
        *active == other.active
            && *title == other.title
            && *artist == other.artist
            && *album == other.album
            && *app == other.app
            && *playing == other.playing
            && *duration_ms == other.duration_ms
            && *art == other.art
    }
}

/// Playback commands sent from the UI to the media thread.
#[derive(Clone, Copy, Debug)]
pub enum Cmd {
    Toggle,
    Next,
    Prev,
    Seek(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_except_position_ignores_only_position() {
        let a = MediaState {
            title: "t".into(),
            art: Some("data:image/png;base64,AA==".into()),
            position_ms: 1_000,
            ..Default::default()
        };
        let mut b = a.clone();
        b.position_ms = 99_000;
        assert!(a.same_except_position(&b));
        b.playing = true;
        assert!(!a.same_except_position(&b));
        b.playing = false;
        b.art = None;
        assert!(!a.same_except_position(&b));
    }
}
