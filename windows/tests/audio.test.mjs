import test from "node:test";
import assert from "node:assert/strict";
import { levelDb } from "../src/hub/audio.ts";

test("audio meters keep silence, invalid input and clipping within a readable scale", () => {
  assert.equal(levelDb(0), -60);
  assert.equal(levelDb(NaN), -60);
  assert.equal(levelDb(-1), -60);
  assert.equal(levelDb(1), 0);
  assert.equal(levelDb(2), 0);
  assert.equal(levelDb(0.001), -60);
  assert.ok(Math.abs(levelDb(0.5) + 6.0206) < 0.001);
});
