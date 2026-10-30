import { test } from "node:test";
import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import ts from "typescript";
import { installFakeDom } from "./fakedom.mjs";
import { calls, emit, internals, sent } from "./tauri.mjs";
import { FlowBridge } from "../src/core/flow.ts";

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

async function exportPage(t) {
  const fixture = await recordingPage(t);
  fixture.page.querySelector(".hub-notes-list").children[0].fire("click");
  await flush();
  return {
    ...fixture,
    exportButton: () => fixture.page.find("button").find(b => b.textContent === "Export as Markdown…"),
    detail: () => fixture.page.querySelector(".hub-note-detail"),
  };
}

test("a pending export opens only one dialog and cancellation reports no saved file", async t => {
  const fixture = await exportPage(t);
  let complete, requests = 0;
  t.mock.method(FlowBridge, "exportNote", () => {
    requests++;
    return new Promise(resolve => { complete = resolve; });
  });
  const button = fixture.exportButton();
  button.fire("click");
  assert.equal(button.disabled, true);
  button.fire("click");
  assert.equal(requests, 1);
  complete(null);
  await flush();
  assert.equal(fixture.exportButton().disabled, false);
  assert.doesNotMatch(fixture.detail().textContent, /Saved as/);
});

test("an export failure is visible and a successful retry reports the selected path", async t => {
  const fixture = await exportPage(t);
  let fail = true;
  t.mock.method(FlowBridge, "exportNote", async () => {
    if (fail) throw new Error("Destination is not writable");
    return "C:\\chosen\\Synthetic call.md";
  });
  fixture.exportButton().fire("click");
  await flush();
  assert.match(fixture.detail().textContent, /Destination is not writable/);
  assert.equal(fixture.exportButton().disabled, false);
  fail = false;
  fixture.exportButton().fire("click");
  await flush();
  assert.doesNotMatch(fixture.detail().textContent, /Destination is not writable/);
  assert.match(fixture.detail().textContent, /Saved as C:\\chosen\\Synthetic call\.md/);
});

test("processing keeps live lines mounted while native stage progress and elapsed time update", async t => {
  const fixture = await recordingPage(t);
  fixture.state.recording = null;
  fixture.state.notes[0].status = "processing";
  fixture.state.working = ["synthetic"];
  const progress = { stage: "microphone", completedSeconds: 30, totalSeconds: 60, startedAt: Date.now() - 65_000, updatedAt: Date.now() };
  fixture.state.progress = { synthetic: progress };
  emit("notes-changed", "synthetic");
  await flush();
  const detail = fixture.page.querySelector(".hub-note-detail");
  const line = detail.querySelector(".hub-line");
  assert.match(line.textContent, /Test/);
  assert.match(detail.textContent, /Live transcript — still being finalized/);
  assert.match(detail.textContent, /Transcribing Microphone/);
  assert.equal(detail.find("div").find(node => node.getAttribute("role") === "progressbar").getAttribute("aria-valuenow"), "50");
  emit("notes-progress", { id: "synthetic", progress: { ...progress, stage: "playback", completedSeconds: 45 } });
  assert.match(detail.textContent, /Transcribing Call playback/);
  assert.equal(detail.find("div").find(node => node.getAttribute("role") === "progressbar").getAttribute("aria-valuenow"), "75");
  fixture.tick();
  assert.equal(detail.querySelector(".hub-line"), line);
  assert.match(detail.textContent, /Processing for 1:05/);
  emit("notes-progress", { id: "synthetic", progress: { ...progress, stage: "speakers", totalSeconds: null } });
  assert.match(detail.textContent, /Identifying speakers/);
  assert.equal(detail.find("div").find(node => node.getAttribute("role") === "progressbar"), undefined, "speaker work has no invented percentage");
  assert.equal(detail.querySelector(".hub-line"), line);
});

test("failed final processing retains the live transcript alongside Retry", async t => {
  const fixture = await recordingPage(t);
  fixture.state.recording = null;
  fixture.state.notes[0].status = "failed";
  fixture.state.notes[0].error = "Could not save call notes";
  emit("notes-changed", "synthetic");
  await flush();
  const detail = fixture.page.querySelector(".hub-note-detail");
  assert.match(detail.textContent, /Could not save call notes/);
  assert.match(detail.querySelector(".hub-line").textContent, /Test/);
  assert.ok(detail.find("button").some(button => button.textContent === "Retry"));
});
