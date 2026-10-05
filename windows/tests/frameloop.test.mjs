// Run with `npm test`. When the island's frame loop may stop: never while
// something on screen is still moving, never while the island is hidden.

import test from "node:test";
import assert from "node:assert/strict";
import { keepRunning } from "../src/core/frameloop.ts";

const still = {
  mode: "expanded",
  geometrySettling: false,
  botSettled: true,
  greeting: false,
  engineBusy: false,
  uploadActive: false,
  viewAnimating: false,
};

test("an island where nothing moves lets the loop stop", () => {
  assert.equal(keepRunning(still), false);
});

test("a ticker scrolling to the next step keeps the loop alive, even when Mochi is still", () => {
  // Measured: in the "working" state Mochi has no looping animation, so the loop
  // stopped right after a step arrived and the ticker froze half-way.
  assert.equal(keepRunning({ ...still, viewAnimating: true }), true);
});

test("each kind of movement keeps it alive while the island is open", () => {
  for (const key of ["geometrySettling", "greeting", "engineBusy", "uploadActive"]) {
    assert.equal(keepRunning({ ...still, [key]: true }), true, key);
  }
  assert.equal(keepRunning({ ...still, botSettled: false }), true, "Mochi moving");
});

test("a hidden island only finishes retracting, whatever its views would do", () => {
  const hidden = { ...still, mode: "hidden", engineBusy: true, viewAnimating: true, botSettled: false };
  assert.equal(keepRunning(hidden), false);
  assert.equal(keepRunning({ ...hidden, geometrySettling: true }), true);
});

test("the compact island does not run the open views' animations", () => {
  assert.equal(keepRunning({ ...still, mode: "compact", viewAnimating: true }), false, "the ticker is not on screen");
  assert.equal(keepRunning({ ...still, mode: "compact", engineBusy: true }), true, "Mochi is");
});
