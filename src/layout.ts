/** Size of the transparent overlay window. Keep in sync with CANVAS_W/H in src-tauri/src/overlay.rs. */
export const CANVAS = { w: 480, h: 220 } as const;

/** Gap between the top of the screen and the pill. */
export const TOP = 10;

export type Mode = "hidden" | "compact" | "peek" | "expanded";

/** Pill dimensions per mode, in logical px. */
export const SIZE: Record<Mode, { w: number; h: number; r: number }> = {
  hidden: { w: 120, h: 22, r: 11 },
  compact: { w: 190, h: 38, r: 19 },
  peek: { w: 340, h: 66, r: 33 },
  expanded: { w: 420, h: 172, r: 38 },
};

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

const PAD = 8;

/**
 * Region the native side should treat as "over the pill" (window coordinates).
 * Outside it the overlay is click-through. Slightly larger than the pill so
 * moving the cursor away doesn't flicker at the edge.
 */
export function hitRect(mode: Mode): Rect {
  const s = SIZE[mode];
  return {
    x: (CANVAS.w - s.w) / 2 - PAD,
    y: 0,
    w: s.w + PAD * 2,
    h: TOP + s.h + PAD,
  };
}

export const NO_HIT: Rect = { x: 0, y: 0, w: 0, h: 0 };
