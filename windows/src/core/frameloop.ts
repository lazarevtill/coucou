// When the island's frame loop may stop. Pure, so it is tested directly.

import type { IslandMode } from "./layout";

export interface LoopState {
  mode: IslandMode;
  /** The island's own size and corners are still animating. */
  geometrySettling: boolean;
  botSettled: boolean;
  greeting: boolean;
  /** Mochi's engine has a tween or a looping animation running. */
  engineBusy: boolean;
  uploadActive: boolean;
  /** The view on screen is animating something of its own (the ticker). */
  viewAnimating: boolean;
}

/**
 * Nothing is drawn while the island is hidden, so then only its retracting may
 * keep the loop alive. Open, anything still moving does — the open view's own
 * animations included: the ticker scrolls on this loop, and in the "working"
 * state Mochi has no looping animation to keep it going.
 */
export function keepRunning(s: LoopState): boolean {
  if (s.mode === "hidden") return s.geometrySettling;
  const moving =
    s.geometrySettling || !s.botSettled || s.greeting || s.engineBusy || s.uploadActive;
  return moving || (s.mode === "expanded" && s.viewAnimating);
}
