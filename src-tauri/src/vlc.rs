//! VLC source via VLC's built-in HTTP interface. VLC 3.x doesn't publish to the
//! Windows media session, so Island talks to it directly on localhost.
//!
//! std-only HTTP/1.1 client: the only server it ever talks to is 127.0.0.1.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value;

use crate::model::{Cmd, MediaState};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
const IO_TIMEOUT: Duration = Duration::from_millis(1500);
const MAX_BODY: usize = 8 * 1024 * 1024;
const MAX_ART_TRIES: u32 = 10;
/// After a failed request (VLC closed, wrong password, ...) don't try again for
/// this long. A refused connection can block for the whole connect timeout on
/// Windows, which would otherwise stall the media thread on every poll.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub password: String,
}

impl Config {
    /// Reads `%APPDATA%\island\config.json` (`~/.config/island/config.json` elsewhere).
    /// On first run a template is written there. `ISLAND_VLC_PASSWORD` overrides the file.
    pub fn load() -> Self {
        let mut cfg = Config {
            port: 8080,
            password: String::new(),
        };
        if let Some(path) = config_path() {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                        if let Some(p) = v
                            .get("vlcPort")
                            .and_then(Value::as_u64)
                            .and_then(|p| u16::try_from(p).ok())
                        {
                            cfg.port = p;
                        }
                        if let Some(s) = v.get("vlcPassword").and_then(Value::as_str) {
                            cfg.password = s.to_string();
                        }
                    }
                }
                Err(_) => {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(
                        &path,
                        "{\n  \"vlcPort\": 8080,\n  \"vlcPassword\": \"\"\n}\n",
                    );
                }
            }
        }
        if let Ok(pw) = std::env::var("ISLAND_VLC_PASSWORD") {
            cfg.password = pw;
        }
        cfg
    }
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("island").join("config.json"))
}

pub struct Vlc {
    cfg: Config,
    track_key: String,
    art: Option<String>,
    art_tries: u32,
    /// Set after a failed request; no requests are made before this instant.
    retry_at: Option<Instant>,
}

impl Vlc {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            track_key: String::new(),
            art: None,
            art_tries: 0,
            retry_at: None,
        }
    }

    /// Current VLC state, or None when VLC isn't running, isn't reachable,
    /// rejects the password, or is stopped.
    pub fn read(&mut self) -> Option<MediaState> {
        if self.retry_at.is_some_and(|t| Instant::now() < t) {
            return None;
        }
        let resp = http_get(&self.cfg, "/requests/status.json").filter(|r| r.status == 200);
        let Some(resp) = resp else {
            self.retry_at = Some(Instant::now() + RETRY_AFTER_FAILURE);
            return None;
        };
        self.retry_at = None;
        let json: Value = serde_json::from_slice(&resp.body).ok()?;
        let st = parse_status(&json)?;

        if st.key != self.track_key {
            self.track_key = st.key.clone();
            self.art = None;
            self.art_tries = 0;
        }
        if self.art.is_none() && self.art_tries < MAX_ART_TRIES {
            self.art_tries += 1;
            self.art = self.fetch_art();
        }

        Some(MediaState {
            active: true,
            title: st.title,
            artist: st.artist,
            album: st.album,
            app: "VLC".into(),
            playing: st.playing,
            position_ms: st.position_ms,
            duration_ms: st.duration_ms,
            art: self.art.clone(),
        })
    }

    pub fn apply(&mut self, cmd: Cmd) {
        let path = match cmd {
            Cmd::Toggle => "/requests/status.json?command=pl_pause".to_string(),
            Cmd::Next => "/requests/status.json?command=pl_next".to_string(),
            Cmd::Prev => "/requests/status.json?command=pl_previous".to_string(),
            Cmd::Seek(ms) => format!("/requests/status.json?command=seek&val={}", ms / 1000),
        };
        let _ = http_get(&self.cfg, &path);
    }

    fn fetch_art(&self) -> Option<String> {
        let resp = http_get(&self.cfg, "/art")?;
        if resp.status != 200 || resp.body.is_empty() {
            return None;
        }
        let mime = if resp.content_type.starts_with("image/") {
            resp.content_type.as_str()
        } else {
            "image/png"
        };
        Some(format!("data:{mime};base64,{}", STANDARD.encode(&resp.body)))
    }
}

struct Status {
    title: String,
    artist: String,
    album: String,
    playing: bool,
    position_ms: u64,
    duration_ms: u64,
    key: String,
}

fn parse_status(v: &Value) -> Option<Status> {
    let state = v.get("state")?.as_str()?;
    if state == "stopped" {
        return None;
    }
    let meta = v.pointer("/information/category/meta");
    let text = |k: &str| {
        meta.and_then(|m| m.get(k))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let mut title = text("title");
    if title.is_empty() {
        title = text("filename");
    }
    let artist = text("artist");
    let album = text("album");
    let ms = |k: &str| (v.get(k).and_then(Value::as_f64).unwrap_or(0.0).max(0.0) * 1000.0) as u64;
    let item = v.get("currentplid").and_then(Value::as_i64).unwrap_or(0);

    Some(Status {
        key: format!("{title}\u{1}{artist}\u{1}{album}\u{1}{item}"),
        title,
        artist,
        album,
        playing: state == "playing",
        position_ms: ms("time"),
        duration_ms: ms("length"),
    })
}

// --- minimal HTTP/1.1 client -------------------------------------------------

struct Response {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

fn http_get(cfg: &Config, path: &str) -> Option<Response> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, cfg.port));
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok()?;

    // VLC's web interface uses Basic auth with an empty user name.
    let auth = STANDARD.encode(format!(":{}", cfg.password));
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Basic {auth}\r\nConnection: close\r\n\r\n",
        cfg.port
    );
    stream.write_all(request.as_bytes()).ok()?;

    // Read until the response is complete, EOF, or a timeout (VLC may keep the
    // socket open even with Connection: close).
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > MAX_BODY + 16 * 1024 {
                    return None;
                }
                if response_complete(&buf) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    parse_response(&buf)
}

fn split_head(buf: &[u8]) -> Option<(String, &[u8])> {
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    Some((
        String::from_utf8_lossy(&buf[..end]).into_owned(),
        &buf[end + 4..],
    ))
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (k, v) = line.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

fn is_chunked(head: &str) -> bool {
    header(head, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
}

fn content_length(head: &str) -> Option<usize> {
    header(head, "content-length").and_then(|v| v.parse().ok())
}

fn response_complete(buf: &[u8]) -> bool {
    let Some((head, body)) = split_head(buf) else {
        return false;
    };
    if is_chunked(&head) {
        return body.ends_with(b"0\r\n\r\n");
    }
    content_length(&head).is_some_and(|n| body.len() >= n)
}

fn parse_response(buf: &[u8]) -> Option<Response> {
    let (head, body) = split_head(buf)?;
    let status = head.lines().next()?.split_whitespace().nth(1)?.parse().ok()?;
    let content_type = header(&head, "content-type").unwrap_or("").to_string();
    let body = if is_chunked(&head) {
        dechunk(body)?
    } else {
        match content_length(&head) {
            Some(n) if n <= body.len() => body[..n].to_vec(),
            _ => body.to_vec(),
        }
    };
    Some(Response {
        status,
        content_type,
        body,
    })
}

fn dechunk(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = data.windows(2).position(|w| w == b"\r\n")?;
        let size_text = std::str::from_utf8(&data[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Some(out);
        }
        if data.len() < size {
            return None;
        }
        out.extend_from_slice(&data[..size]);
        data = data.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    const STATUS: &str = r#"{"state":"playing","time":51,"length":213,"currentplid":5,
        "information":{"category":{"meta":{"title":"Song","artist":"Band","album":"LP","filename":"song.mp3"}}}}"#;

    /// Fake VLC: checks the Basic auth header, serves canned JSON and art, and
    /// records every request line.
    fn serve(password: &'static str, chunked: bool) -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = req.lines().next().unwrap_or("").to_string();
                seen.lock().unwrap().push(line.clone());

                let want = format!("Authorization: Basic {}", STANDARD.encode(format!(":{password}")));
                let (code, ctype, body): (&str, &str, Vec<u8>) = if !req.contains(&want) {
                    ("401 Unauthorized", "text/plain", b"no".to_vec())
                } else if line.contains("/art") {
                    ("200 OK", "image/png", vec![1, 2, 3, 4])
                } else {
                    ("200 OK", "application/json", STATUS.as_bytes().to_vec())
                };
                let reply = if chunked {
                    let mut out = format!(
                        "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nTransfer-Encoding: chunked\r\n\r\n"
                    )
                    .into_bytes();
                    for part in body.chunks(7) {
                        out.extend_from_slice(format!("{:x}\r\n", part.len()).as_bytes());
                        out.extend_from_slice(part);
                        out.extend_from_slice(b"\r\n");
                    }
                    out.extend_from_slice(b"0\r\n\r\n");
                    out
                } else {
                    let mut out = format!(
                        "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    out.extend_from_slice(&body);
                    out
                };
                let _ = s.write_all(&reply);
            }
        });
        (port, log)
    }

    fn cfg(port: u16, password: &str) -> Config {
        Config {
            port,
            password: password.into(),
        }
    }

    #[test]
    fn reads_state_and_art() {
        let (port, _) = serve("pw", false);
        let s = Vlc::new(cfg(port, "pw")).read().expect("state");
        assert!(s.active && s.playing);
        assert_eq!((s.title.as_str(), s.artist.as_str(), s.album.as_str()), ("Song", "Band", "LP"));
        assert_eq!((s.position_ms, s.duration_ms), (51_000, 213_000));
        assert_eq!(s.app, "VLC");
        assert_eq!(s.art.as_deref(), Some("data:image/png;base64,AQIDBA=="));
    }

    #[test]
    fn handles_chunked_responses() {
        let (port, _) = serve("pw", true);
        let s = Vlc::new(cfg(port, "pw")).read().expect("state");
        assert_eq!(s.title, "Song");
        assert!(s.art.is_some());
    }

    #[test]
    fn wrong_password_or_no_server_gives_none() {
        let (port, _) = serve("pw", false);
        assert!(Vlc::new(cfg(port, "nope")).read().is_none());
        let free = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert!(Vlc::new(cfg(free, "pw")).read().is_none());
    }

    #[test]
    fn commands_hit_the_right_endpoints() {
        let (port, log) = serve("pw", false);
        let mut vlc = Vlc::new(cfg(port, "pw"));
        vlc.apply(Cmd::Toggle);
        vlc.apply(Cmd::Next);
        vlc.apply(Cmd::Prev);
        vlc.apply(Cmd::Seek(95_400));
        let lines = log.lock().unwrap().clone();
        let expect = [
            "GET /requests/status.json?command=pl_pause HTTP/1.1",
            "GET /requests/status.json?command=pl_next HTTP/1.1",
            "GET /requests/status.json?command=pl_previous HTTP/1.1",
            "GET /requests/status.json?command=seek&val=95 HTTP/1.1",
        ];
        assert_eq!(lines, expect);
    }

    #[test]
    fn backs_off_after_a_failure_then_recovers() {
        let free = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut vlc = Vlc::new(cfg(free, "pw"));
        assert!(vlc.read().is_none());
        assert!(vlc.retry_at.is_some(), "failure should schedule a retry");

        // Even with a server now listening, nothing is requested until the backoff ends.
        let (port, log) = serve("pw", false);
        vlc.cfg.port = port;
        assert!(vlc.read().is_none());
        assert!(log.lock().unwrap().is_empty());

        vlc.retry_at = Some(Instant::now());
        assert!(vlc.read().is_some());
        assert!(vlc.retry_at.is_none());
    }

    #[test]
    fn stopped_and_filename_fallback() {
        let stopped: Value = serde_json::from_str(r#"{"state":"stopped"}"#).unwrap();
        assert!(parse_status(&stopped).is_none());
        let bare: Value = serde_json::from_str(
            r#"{"state":"paused","time":3,"length":10,"information":{"category":{"meta":{"filename":"a.mp3"}}}}"#,
        )
        .unwrap();
        let s = parse_status(&bare).unwrap();
        assert_eq!(s.title, "a.mp3");
        assert!(!s.playing);
    }
}
