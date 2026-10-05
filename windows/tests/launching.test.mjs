// Run with `npm test`. What the island says while an editor or a terminal is
// being opened, so a slow start never looks like nothing happened.

import test from "node:test";
import assert from "node:assert/strict";
import { launcherName, openNote, startedUntil, stillStarting } from "../src/core/launching.ts";

const ok = (via, startingMs = 8000) => ({ via, fellBack: false, error: null, startingMs });

test("a launch that started says it is on its way", () => {
  assert.equal(openNote(ok("cursor"), "Cursor"), "Opening Cursor…");
  assert.equal(openNote(ok("windowsTerminal"), "Windows Terminal"), "Opening Windows Terminal…");
});

test("a second click while it starts says so instead of opening another one", () => {
  assert.equal(openNote(ok("starting", 5000), "Cursor"), "Cursor is already starting…");
});

test("a fallback and a failure say what happened", () => {
  const fell = { via: "explorer", fellBack: true, error: "The chosen editor was not found.", startingMs: 8000 };
  assert.equal(openNote(fell, "Cursor"), "The chosen editor was not found. The folder opened in File Explorer instead.");
  const failed = { via: "none", fellBack: false, error: "There is no project folder to open.", startingMs: null };
  assert.equal(openNote(failed, "Windows Terminal"), "There is no project folder to open.");
  assert.equal(openNote({ via: "none", fellBack: false, error: null, startingMs: null }, "Cursor"), null, "switched off: nothing to say");
  assert.equal(openNote(null, "Cursor"), null, "no answer from the app");
});

test("the button stays 'opening' for as long as the app counts the launch as starting", () => {
  const now = 1_000_000;
  const until = startedUntil(ok("cursor", 8000), now);
  assert.equal(until, now + 8000);
  assert.equal(startedUntil(ok("starting", 3000), now), now + 3000);
  assert.equal(startedUntil({ via: "none", fellBack: false, error: "x", startingMs: null }, now), null);
  assert.equal(stillStarting(until, now + 7999), true);
  assert.equal(stillStarting(until, now + 8000), false);
  assert.equal(stillStarting(null, now), false);
});

test("the launcher is named as the settings choose it", () => {
  assert.equal(launcherName("editor", { editor: "cursor", terminal: "windowsTerminal" }), "Cursor");
  assert.equal(launcherName("editor", { editor: "vscode", terminal: "windowsTerminal" }), "VS Code");
  assert.equal(launcherName("terminal", { editor: "cursor", terminal: "windowsTerminal" }), "Windows Terminal");
  assert.equal(launcherName("terminal", { editor: "cursor", terminal: "shell" }), "PowerShell");
  assert.equal(launcherName("editor", { editor: "none", terminal: "shell" }), null, "switched off");
});
