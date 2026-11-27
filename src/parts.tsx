import { useEffect, useRef, type MouseEvent } from "react";
import type { MediaState } from "./types";

export function fmt(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** "Spotify.exe" -> "Spotify", "Microsoft.ZuneMusic_xyz!App" -> "ZuneMusic". */
export function appLabel(id: string): string {
  const tail = id.split("!").pop() ?? id;
  const base =
    tail
      .replace(/\.exe$/i, "")
      .split(/[._]/)
      .filter(Boolean)
      .pop() ?? tail;
  return base.charAt(0).toUpperCase() + base.slice(1);
}

export function Art({ src, size, radius }: { src: string | null; size: number; radius: number }) {
  return (
    <div className="art" style={{ width: size, height: size, borderRadius: radius }}>
      {src ? (
        <img src={src} alt="" draggable={false} />
      ) : (
        <svg viewBox="0 0 24 24" width={size * 0.5} height={size * 0.5} fill="currentColor" aria-hidden>
          <path d="M9 17.5V6.6l9-1.9v10.8a2.5 2.5 0 1 1-1.5-2.3V7.4L10.5 8.6v8.9A2.5 2.5 0 1 1 9 17.5Z" />
        </svg>
      )}
    </div>
  );
}

/** Decorative bars (not driven by real audio). */
export function Equalizer({ playing }: { playing: boolean }) {
  return (
    <div className={playing ? "eq on" : "eq"} aria-hidden>
      <i />
      <i />
      <i />
      <i />
    </div>
  );
}

export function PauseGlyph() {
  return (
    <svg className="glyph" viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden>
      <rect x="6" y="5" width="4" height="14" rx="1.2" />
      <rect x="14" y="5" width="4" height="14" rx="1.2" />
    </svg>
  );
}

interface ProgressProps {
  state: MediaState;
  receivedAt: number;
  /** Only animate while visible. */
  running: boolean;
  onSeek: (positionMs: number) => void;
}

export function Progress({ state, receivedAt, running, onSeek }: ProgressProps) {
  const fill = useRef<HTMLDivElement>(null);
  const label = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    const paint = () => {
      const elapsed = state.playing ? performance.now() - receivedAt : 0;
      let pos = state.positionMs + elapsed;
      if (state.durationMs > 0) pos = Math.min(pos, state.durationMs);
      const frac = state.durationMs > 0 ? pos / state.durationMs : 0;
      if (fill.current) fill.current.style.transform = `scaleX(${frac})`;
      if (label.current) label.current.textContent = fmt(pos);
    };
    paint();
    if (!running || !state.playing) return;
    let raf = requestAnimationFrame(function loop() {
      paint();
      raf = requestAnimationFrame(loop);
    });
    return () => cancelAnimationFrame(raf);
  }, [state, receivedAt, running]);

  const seekable = state.durationMs > 0;
  const onClick = (e: MouseEvent<HTMLDivElement>) => {
    if (!seekable) return;
    const box = e.currentTarget.getBoundingClientRect();
    const frac = Math.min(1, Math.max(0, (e.clientX - box.left) / box.width));
    onSeek(Math.round(frac * state.durationMs));
  };

  return (
    <div className="progress">
      <div className={seekable ? "track seekable" : "track"} onClick={onClick}>
        <div className="bar">
          <div className="fill" ref={fill} />
        </div>
      </div>
      <div className="times">
        <span ref={label}>0:00</span>
        <span>{seekable ? fmt(state.durationMs) : "live"}</span>
      </div>
    </div>
  );
}

interface ControlsProps {
  playing: boolean;
  onPrev: () => void;
  onToggle: () => void;
  onNext: () => void;
}

export function Controls({ playing, onPrev, onToggle, onNext }: ControlsProps) {
  return (
    <div className="controls">
      <button type="button" onClick={onPrev} aria-label="Previous">
        <svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor" aria-hidden>
          <path d="M6 5h2v14H6zM20 5.5v13a.6.6 0 0 1-.95.48L10.4 13a1 1 0 0 1 0-1.64l8.65-6.34A.6.6 0 0 1 20 5.5Z" />
        </svg>
      </button>
      <button type="button" className="main" onClick={onToggle} aria-label={playing ? "Pause" : "Play"}>
        {playing ? (
          <svg viewBox="0 0 24 24" width="24" height="24" fill="currentColor" aria-hidden>
            <rect x="6" y="5" width="4" height="14" rx="1.2" />
            <rect x="14" y="5" width="4" height="14" rx="1.2" />
          </svg>
        ) : (
          <svg viewBox="0 0 24 24" width="24" height="24" fill="currentColor" aria-hidden>
            <path d="M8 5.7v12.6a.9.9 0 0 0 1.38.76l9.9-6.3a.9.9 0 0 0 0-1.52l-9.9-6.3A.9.9 0 0 0 8 5.7Z" />
          </svg>
        )}
      </button>
      <button type="button" onClick={onNext} aria-label="Next">
        <svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor" aria-hidden>
          <path d="M16 5h2v14h-2zM4 5.5v13a.6.6 0 0 0 .95.48L13.6 13a1 1 0 0 0 0-1.64L4.95 5.02A.6.6 0 0 0 4 5.5Z" />
        </svg>
      </button>
    </div>
  );
}
