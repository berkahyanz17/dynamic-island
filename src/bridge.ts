import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** False when the UI runs in a plain browser (`npm run dev`), where there is no native side. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Call a Rust command. Resolves to undefined outside Tauri or on error. */
export function call<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T | undefined> {
  if (!inTauri) return Promise.resolve(undefined);
  return invoke<T>(cmd, args).catch(() => undefined);
}

/** Subscribe to a Rust event. Returns the unsubscribe function. */
export function on<T>(event: string, handler: (payload: T) => void): () => void {
  if (!inTauri) return () => {};
  let off: (() => void) | undefined;
  let disposed = false;
  listen<T>(event, (e) => handler(e.payload)).then((unlisten) => {
    if (disposed) unlisten();
    else off = unlisten;
  });
  return () => {
    disposed = true;
    off?.();
  };
}
