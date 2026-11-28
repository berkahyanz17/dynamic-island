# Island

A dynamic-island style media overlay for Windows. Tauri 2 (Rust) + React + TypeScript + Motion.

It follows whatever app Windows treats as the active media session (Spotify, browsers, Windows
Media Player, ...) plus VLC through its web interface (VLC 3.x doesn't publish to Windows' media
controls), and shows it in a pill pinned to the top centre of the primary monitor.

## States

| State | When |
|---|---|
| compact | something is playing: cover + equaliser |
| peek | new track, or play/pause flipped: cover + title + artist for ~3.4 s |
| expanded | cursor over the pill: cover, title, artist, seek bar, prev / play-pause / next |
| hidden | nothing playing, or paused for 6+ s (hover the top edge to bring it back) |

The overlay is click-through everywhere except over the pill, never takes focus, stays out of
Alt-Tab and the taskbar, and hides itself while a fullscreen app or game is running.
Tray icon: show/hide, quit.

## Prerequisites (Windows)

- Node.js 20+
- Rust (stable, MSVC toolchain) and the "Desktop development with C++" workload from Visual Studio Build Tools
- WebView2 runtime (already on Windows 11 and up-to-date Windows 10)

## Run

```powershell
npm install
npm run tauri dev       # dev build with hot reload
npm run tauri build     # installer in src-tauri/target/release/bundle/nsis
```

`npm run dev` alone opens the UI in a normal browser with a fake track, which is handy for
tweaking the animation without compiling Rust.

## VLC setup

1. VLC > Tools > Preferences > Show settings: All > Interface > Main interfaces: tick **Web**.
2. Same tree, Main interfaces > **Lua** > Lua HTTP > set a **Password**. Restart VLC.
3. Put the same password in `%APPDATA%\island\config.json` (Island writes a template there on first run):

   ```json
   { "vlcPort": 8080, "vlcPassword": "your-password" }
   ```

   `ISLAND_VLC_PASSWORD` in the environment overrides the file. Restart Island after editing.

Which source wins: the Windows media session wins ties. VLC takes over when it is playing and the
system session isn't (or the system has nothing). Island polls `127.0.0.1` only; if VLC isn't
running or the password is wrong, VLC is silently skipped.

## Layout

```
src/                     React UI
  Island.tsx             state machine + morphing pill (Motion springs)
  parts.tsx              cover art, equaliser, progress bar, controls
  layout.ts              pill sizes per state, hit-test rect  <- tweak sizes here
  useMedia.ts            subscribes to the native media state
  bridge.ts              invoke/listen wrappers (no-ops outside Tauri)
src-tauri/src/
  gsmtc.rs               Windows media session reader + playback commands (WinRT)
  win32.rs               overlay window styles, fullscreen detection
  vlc.rs                 VLC source over its HTTP interface (std-only client, has tests)
  media.rs               media thread: poll both sources, emit on change, Tauri commands
  overlay.rs             placement, click-through hit testing, tray
  model.rs               shared state/command types
  demo.rs                fake media source for non-Windows builds
```

## How it fits together

- `media.rs` polls the media session every 400 ms on its own thread and emits `media://state` only
  when the track, play state or cover changes, or the position jumps (a seek). Between events the UI
  extrapolates the progress bar itself.
- The window is a fixed transparent 480x220 canvas. `overlay.rs` tracks the cursor and toggles
  click-through depending on whether it is inside the pill's rectangle (reported by the UI via
  `set_hit_rect`), and sends `island://over` so the UI knows when to expand.
- The cursor check reads the cursor and window rectangle straight from Win32 and ticks every 16 ms only
  when the cursor is within ~120 px of the pill (60 ms otherwise, 250 ms when there is no media), so the
  idle cost is near zero. VLC is skipped for 2 s after any failed request instead of being retried every poll.
- If you change the canvas size, change it in both `overlay.rs` (`CANVAS_W/H`) and `src/layout.ts`.

## Notes

- Cover art is sent to the UI as a base64 data URL with every state event, including seeks.
- The equaliser is decorative. A real one needs WASAPI loopback capture on the Rust side.
- Icons in `src-tauri/icons` are placeholders; replace them with `npx tauri icon your-logo.png`.
- Controls depend on what the source app supports (some browsers ignore seek or previous).
#
