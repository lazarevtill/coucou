import { test } from "node:test";
import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import ts from "typescript";
import { installFakeDom } from "./fakedom.mjs";
import { calls, emit, internals, sent } from "./tauri.mjs";

const { root } = installFakeDom();
// The page imports the real animated hero, whose parameter property needs a
// TypeScript transform rather than Node's built-in type stripping.
registerHooks({ load(url, context, nextLoad) {
  const loaded = nextLoad(url, context);
  if (!url.endsWith("/src/hub/hero.ts")) return loaded;
  return { ...loaded, format: "module", source: ts.transpileModule(String(loaded.source), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText };
} });
document.addEventListener = () => {};
document.documentElement = { lang: "en" };
document.visibilityState = "hidden";
globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
Object.getPrototypeOf(root()).prepend = function (...nodes) {
  for (const node of nodes) node.parentNode = this;
  this.childNodes.unshift(...nodes);
};
const { notesPage } = await import("../src/hub/pages/notes.ts");
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
const originalInvoke = internals.invoke;

async function recordingPage(t) {
  calls.length = 0;
  let tick;
  t.mock.method(globalThis, "setInterval", fn => { tick = fn; return 1; });
  t.mock.method(globalThis, "clearInterval", () => {});
  const note = {
    id: "synthetic", title: "Synthetic call", start: Date.now() - 10_000,
    status: "recording", speakers: [], lines: [{ start: 0, speaker: "me", text: "Test" }],
    summary: "", keyPoints: [], todos: [],
  };
  const state = { notes: [note], recording: note.id, working: [], audio: [] };
  let stop = async () => { state.recording = null; note.status = "ready"; };
  t.mock.method(internals, "invoke", async (cmd, args) => {
    const fallback = await originalInvoke(cmd, args);
    if (cmd === "notes_state") return structuredClone(state);
    if (cmd === "notes_stop") return stop();
    return fallback;
  });
  const page = root();
  const ctx = { hero: () => root(), status: { models: { parakeet: { state: "ready" } } } };
  await notesPage(page, ctx);
  t.after(() => ctx.onLeave());
  return { page, state, tick: () => tick(), stop: fn => { stop = fn; } };
}

test("the Stop button survives clock ticks and note updates, then becomes Record in place", async t => {
  const fixture = await recordingPage(t);
  const control = fixture.page.querySelector(".hub-notes-list").children[0];
  assert.match(control.textContent, /Stop/);
  fixture.tick();
  await flush();
  assert.equal(fixture.page.querySelector(".hub-notes-list").children[0], control);
  emit("notes-changed", "synthetic");
  await flush();
  assert.equal(fixture.page.querySelector(".hub-notes-list").children[0], control);
  control.fire("click");
  await flush();
  assert.equal(sent("notes_stop").length, 1);
  assert.equal(fixture.page.querySelector(".hub-notes-list").children[0], control);
  assert.equal(control.textContent, "Record a call");
  assert.equal(control.disabled, false);
});

test("a pending stop cannot be submitted twice and a failed stop stays visible and retryable", async t => {
  const fixture = await recordingPage(t);
  const control = fixture.page.querySelector(".hub-notes-list").children[0];
  let reject;
  fixture.stop(() => new Promise((_, fail) => { reject = fail; }));
  control.fire("click");
  await flush();
  assert.equal(control.disabled, true);
  fixture.tick();
  control.fire("click");
  assert.equal(sent("notes_stop").length, 1);
  reject(new Error("Could not stop capture"));
  await flush();
  assert.equal(control.disabled, false);
  assert.match(control.textContent, /Stop/);
  const error = fixture.page.querySelector(".hub-notes-list").querySelector(".hub-warn");
  assert.equal(error.hidden, false);
  assert.equal(error.textContent, "Could not stop capture");
  fixture.stop(async () => { fixture.state.recording = null; fixture.state.notes[0].status = "ready"; });
  control.fire("click");
  await flush();
  assert.equal(sent("notes_stop").length, 2);
  assert.equal(control.textContent, "Record a call");
  assert.equal(error.hidden, true);
});
