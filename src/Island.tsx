import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { motion } from "motion/react";
import { call, on } from "./bridge";
import { hitRect, NO_HIT, SIZE, TOP, type Mode } from "./layout";
import { appLabel, Art, Controls, Equalizer, PauseGlyph, Progress } from "./parts";
import { useMedia } from "./useMedia";

const SPRING = { type: "spring", stiffness: 420, damping: 34, mass: 0.8 } as const;
/** How long the pill pops open on a new track or play/pause change. */
const PEEK_MS = 3400;
/** A paused pill stays visible this long, then tucks away. */
const PAUSE_LINGER_MS = 6000;
/** Grace period before collapsing, so brushing the edge doesn't flicker. */
const LEAVE_DELAY_MS = 220;

export default function Island() {
  const { state, receivedAt, toggle, next, prev, seek } = useMedia();
  const [hover, setHover] = useState(false);
  const [peek, setPeek] = useState(false);
  const [idle, setIdle] = useState(false);
  const leaveTimer = useRef<number | undefined>(undefined);

  const setOver = useCallback((over: boolean) => {
    window.clearTimeout(leaveTimer.current);
    if (over) setHover(true);
    else leaveTimer.current = window.setTimeout(() => setHover(false), LEAVE_DELAY_MS);
  }, []);

  // The window is click-through, so the webview can't see the cursor on its own:
  // the native side tracks it and tells us when it is over the pill.
  useEffect(() => on<boolean>("island://over", setOver), [setOver]);

  const { active } = state;
  const trackKey = `${state.app}|${state.title}|${state.artist}`;

  // Pop open briefly on a new track or when play/pause flips.
  useEffect(() => {
    if (!active) return;
    setPeek(true);
    const t = window.setTimeout(() => setPeek(false), PEEK_MS);
    return () => window.clearTimeout(t);
  }, [active, trackKey, state.playing]);

  // Tuck away after lingering paused.
  useEffect(() => {
    if (!active || state.playing) {
      setIdle(false);
      return;
    }
    const t = window.setTimeout(() => setIdle(true), PAUSE_LINGER_MS);
    return () => window.clearTimeout(t);
  }, [active, state.playing]);

  const mode: Mode = !active
    ? "hidden"
    : hover
      ? "expanded"
      : peek
        ? "peek"
        : idle
          ? "hidden"
          : "compact";

  // Tell the native side where the pill is going to be.
  useEffect(() => {
    void call("set_hit_rect", { ...(active ? hitRect(mode) : NO_HIT) });
  }, [mode, active]);

  const s = SIZE[mode];
  const hidden = mode === "hidden";

  const status = state.playing ? <Equalizer playing /> : <PauseGlyph />;

  return (
    <div className="stage" style={{ paddingTop: TOP }} onContextMenu={(e) => e.preventDefault()}>
      <motion.div
        className="pill"
        initial={false}
        animate={{
          width: s.w,
          height: s.h,
          borderRadius: s.r,
          opacity: hidden ? 0 : 1,
          y: hidden ? -8 : 0,
        }}
        transition={{ default: SPRING, opacity: { duration: 0.2 } }}
        onPointerEnter={() => setOver(true)}
        onPointerLeave={() => setOver(false)}
      >
        <Layer show={mode === "compact"} w={SIZE.compact.w} h={SIZE.compact.h}>
          <div className="row compact">
            <Art src={state.art} size={24} radius={8} />
            {status}
          </div>
        </Layer>

        <Layer show={mode === "peek"} w={SIZE.peek.w} h={SIZE.peek.h}>
          <div className="row peek">
            <Art src={state.art} size={46} radius={14} />
            <div className="meta">
              <div className="title">{state.title || "Unknown title"}</div>
              <div className="artist">{state.artist || appLabel(state.app)}</div>
            </div>
            {status}
          </div>
        </Layer>

        <Layer show={mode === "expanded"} w={SIZE.expanded.w} h={SIZE.expanded.h}>
          <div className="exp">
            <div className="exp-top">
              <Art src={state.art} size={64} radius={14} />
              <div className="meta">
                <div className="title big">{state.title || "Unknown title"}</div>
                <div className="artist">{state.artist || state.album || "Unknown artist"}</div>
                <div className="app">{appLabel(state.app)}</div>
              </div>
              <Equalizer playing={state.playing} />
            </div>
            <Progress state={state} receivedAt={receivedAt} running={mode === "expanded"} onSeek={seek} />
            <Controls playing={state.playing} onPrev={prev} onToggle={toggle} onNext={next} />
          </div>
        </Layer>
      </motion.div>
    </div>
  );
}

/**
 * One view of the pill. Every view has a fixed size and is centred inside the
 * (clipped) pill, so the pill's resize reveals content instead of reflowing it.
 */
function Layer({ show, w, h, children }: { show: boolean; w: number; h: number; children: ReactNode }) {
  return (
    <motion.div
      className="layer"
      style={{ width: w, height: h, pointerEvents: show ? "auto" : "none" }}
      initial={false}
      animate={{ opacity: show ? 1 : 0, scale: show ? 1 : 0.96 }}
      transition={{ duration: show ? 0.2 : 0.1, delay: show ? 0.08 : 0 }}
      aria-hidden={!show}
    >
      {children}
    </motion.div>
  );
}
