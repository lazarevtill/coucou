import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { State, DEFAULT_SETTINGS } from "../src/core/state.ts";
import { Bridge } from "../src/core/bridge.ts";
import { runGlobalShortcut } from "../src/island/shortcuts.ts";
import { Recap } from "../src/recap/recap.ts";
import { Updates } from "../src/island/update.ts";
import { NO_UPDATE } from "../src/core/update.ts";

let requested, background, keyboard;
const host = {
  openExplicit: view => requested.push(view),
  // A disabled island ignores background alerts, just as the real host does.
  alert: view => { if (State.settings.island !== "off") background.push(view); },
  collapse() {}, emote() {}, setView() {}, setPinned() {},
  takeKeyboard: () => keyboard++,
};
const resume = () => { State.paused = false; };

beforeEach(() => {
  requested = []; background = []; keyboard = 0;
  State.settings = { ...DEFAULT_SETTINGS, island: "off" };
  State.mode = "hidden";
  State.view = "overview";
  State.paused = false;
  State.pendingApproval = null;
  State.stateOverride = null;
  State.tasks = [];
  State.focusId = null;
  State.chatHistory = [];
  State.promptContext = null;
});

test("Open Chat and the island toggle work with the idle island switched off", () => {
  runGlobalShortcut(host, "openChat", resume);
  assert.deepEqual(requested, ["prompt"]);
  runGlobalShortcut(host, "toggleIsland", resume);
  assert.equal(requested.length, 2);
  assert.equal(keyboard, 1);
  assert.equal(State.settings.island, "off");
});

test("the waiting-card shortcut explicitly opens its card without deciding it", () => {
  State.pendingApproval = { requestId: "test", sessionId: "test", pillId: "integration_codex", tool: "Read" };
  runGlobalShortcut(host, "goToAlert", resume);
  assert.deepEqual(requested, ["approval"]);
  assert.equal(State.pendingApproval.requestId, "test");
  assert.equal(keyboard, 1);
});

test("front-window attachment opens its preview while the idle island is off", async t => {
  t.mock.method(Bridge, "chatFrontWindowContext", async () => ({ appName: "Test", title: "Synthetic window" }));
  t.mock.method(Bridge, "chatReset", async () => {});
  runGlobalShortcut(host, "attachFrontWindow", resume);
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.deepEqual(requested, ["prompt"]);
  assert.equal(State.promptContext.title, "Synthetic window");
  assert.equal(keyboard, 1);
});

test("a requested recap opens when automatic island alerts are disabled", async () => {
  await Recap.open(host);
  assert.deepEqual(requested, ["recap"]);
  assert.deepEqual(background, []);
});

test("a requested update opens while automatic announcements stay suppressed", async () => {
  await Updates.register(host);
  Updates.status = { ...NO_UPDATE, latest: { version: "9.0.0" } };
  Updates.offer("announce");
  assert.deepEqual(requested, []);
  assert.deepEqual(background, []);
  Updates.show();
  assert.deepEqual(requested, ["update"]);
});
