import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { call, inTauri, on } from "./bridge";
import { EMPTY_STATE, type MediaState } from "./types";

// A fake track so the UI can be designed in a plain browser (`npm run dev`).
const DEMO_ART =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
       <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
         <stop offset="0" stop-color="#ff7a59"/><stop offset="1" stop-color="#7b2ff7"/>
       </linearGradient></defs>
       <rect width="64" height="64" fill="url(#g)"/>
     </svg>`,
  );

const DEMO_STATE: MediaState = {
  active: true,
  title: "Demo Track",
  artist: "Island",
  album: "Preview",
  app: "demo",
  playing: true,
  positionMs: 38_000,
  durationMs: 214_000,
  art: DEMO_ART,
};

export interface MediaApi {
  state: MediaState;
  /** `performance.now()` at the moment `state` arrived; lets the UI extrapolate the position. */
  receivedAt: number;
  toggle: () => void;
  next: () => void;
  prev: () => void;
  seek: (positionMs: number) => void;
}

export function useMedia(): MediaApi {
  const [state, setState] = useState<MediaState>(inTauri ? EMPTY_STATE : DEMO_STATE);
  const [receivedAt, setReceivedAt] = useState(() => performance.now());
  const latest = useRef({ state, receivedAt });
  latest.current = { state, receivedAt };

  const apply = useCallback((next: MediaState) => {
    setState(next);
    setReceivedAt(performance.now());
  }, []);

  useEffect(() => {
    if (!inTauri) return;
    let stale = false;
    call<MediaState>("get_state").then((s) => {
      if (!stale && s) apply(s);
    });
    const off = on<MediaState>("media://state", apply);
    return () => {
      stale = true;
      off();
    };
  }, [apply]);

  return useMemo<MediaApi>(() => {
    // Browser-only: emulate what the native side would report.
    const demo = (patch: (s: MediaState, now: number) => Partial<MediaState>) => {
      const { state: s, receivedAt: at } = latest.current;
      const now = Math.min(s.durationMs, s.positionMs + (s.playing ? performance.now() - at : 0));
      apply({ ...s, ...patch(s, now) });
    };
    return {
      state,
      receivedAt,
      toggle: () => (inTauri ? void call("media_toggle") : demo((s, now) => ({ playing: !s.playing, positionMs: now }))),
      next: () => (inTauri ? void call("media_next") : demo(() => ({ positionMs: 0 }))),
      prev: () => (inTauri ? void call("media_prev") : demo(() => ({ positionMs: 0 }))),
      seek: (positionMs) =>
        inTauri ? void call("media_seek", { positionMs }) : demo(() => ({ positionMs })),
    };
  }, [state, receivedAt, apply]);
}
