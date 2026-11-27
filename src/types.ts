/** Mirrors `MediaState` in src-tauri/src/model.rs (camelCase over the wire). */
export interface MediaState {
  active: boolean;
  title: string;
  artist: string;
  album: string;
  app: string;
  playing: boolean;
  positionMs: number;
  durationMs: number;
  /** `data:` URL of the cover art, if the source app provides one. */
  art: string | null;
}

export const EMPTY_STATE: MediaState = {
  active: false,
  title: "",
  artist: "",
  album: "",
  app: "",
  playing: false,
  positionMs: 0,
  durationMs: 0,
  art: null,
};
