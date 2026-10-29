// The chat view (src/views/chat.ts) on a fake DOM, through the real bridge:
// the model switcher asks for a provider's models only once it is picked and
// has a key, picking saves the settings, and a streamed answer grows in place.

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { calls, emit, internals, sent } from "./tauri.mjs";

installFakeDom();
const { buildPrompt } = await import("../src/views/chat.ts");
const { DEFAULT_SETTINGS, State } = await import("../src/core/state.ts");

/** What the mocked Rust side answers, by command. */
let answers;
const plainInvoke = internals.invoke;
internals.invoke = async (cmd, args) => {
  const result = await plainInvoke(cmd, args);
  if (cmd in answers) return typeof answers[cmd] === "function" ? answers[cmd](args) : answers[cmd];
  return result;
};

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

let view;
beforeEach(() => {
  calls.length = 0;
  answers = {};
  State.settings = { ...DEFAULT_SETTINGS, chatModels: {} };
  State.chatHistory = [];
  State.stateOverride = null;
  State.view = "prompt";
  State.droppedFile = null;
  State.promptContext = null;
  view = buildPrompt(() => {});
  view.sync();
});

const $ = (cls) => view.el.querySelector(cls);
const chips = () => view.el.find(".picker-chip").map((c) => c.textContent);
const models = () => view.el.find(".picker-model").map((m) => m.textContent);

test("the model button shows the active provider's model", () => {
  assert.equal($(".model-name").textContent, "claude-opus-5");
  State.settings = { ...State.settings, chatProvider: "google" };
  view.sync();
  assert.equal($(".model-name").textContent, "gemini-2.0-flash");
  State.settings = { ...State.settings, chatProvider: "ollama" };
  view.sync();
  assert.equal($(".model-name").textContent, "Choose a model");
});

test("a provider without a key is never asked for its models", async () => {
  answers.secret_present = false;
  $(".model-btn").fire("click");
  await flush();
  assert.ok($(".chat-body").classList.contains("picking"));
  assert.deepEqual(chips(), ["Claude Code", "Codex", "Anthropic", "Google", "OpenAI", "OpenRouter"]);
  assert.deepEqual(sent("secret_present"), [{ key: "anthropic-api-key" }]);
  assert.deepEqual(sent("chat_models"), []);
  assert.match($(".picker-status").textContent, /No API key/);
});

test("with a key, the models are listed and picking one saves it", async () => {
  answers.secret_present = true;
  answers.chat_models = [
    { id: "claude-opus-5", label: "Claude Opus 5" },
    { id: "claude-sonnet-5", label: "Claude Sonnet 5" },
  ];
  $(".model-btn").fire("click");
  await flush();
  assert.deepEqual(sent("chat_models"), [{ provider: "anthropic" }]);
  assert.deepEqual(models(), ["Claude Opus 5", "Claude Sonnet 5"]);
  view.el.find(".picker-model")[1].fire("click");
  assert.equal(State.settings.model, "claude-sonnet-5");
  assert.equal(sent("save_settings").at(-1).settings.model, "claude-sonnet-5");
  assert.ok(!$(".chat-body").classList.contains("picking"));
  assert.equal($(".model-name").textContent, "claude-sonnet-5");
});

test("switching provider saves it and asks the new provider only", async () => {
  answers.secret_present = (args) => args.key === "google-api-key";
  answers.chat_models = (args) => (args.provider === "google" ? [{ id: "gemini-2.5-flash", label: "gemini-2.5-flash" }] : []);
  $(".model-btn").fire("click");
  await flush();
  assert.deepEqual(sent("chat_models"), []);
  view.el.find(".picker-chip").find((chip) => chip.textContent === "Google").fire("click");
  await flush();
  assert.equal(State.settings.chatProvider, "google");
  assert.deepEqual(sent("chat_models"), [{ provider: "google" }]);
  // The saved model was not offered: the flash one is kept instead, and saved.
  assert.equal(State.settings.chatModels.google, "gemini-2.5-flash");
  assert.equal(sent("save_settings").at(-1).settings.chatModels.google, "gemini-2.5-flash");
  assert.deepEqual(models(), ["gemini-2.5-flash"]);
});

test("a local answer streams into one reply, then the finished text replaces it", async () => {
  let finish;
  answers.chat_send = () => new Promise((resolve) => (finish = resolve));
  const input = $(".chat-input");
  input.value = "hello";
  $(".send-btn").fire("click");
  await flush();
  view.sync(); // what State.notify() does in the island
  assert.ok($(".model-btn").disabled, "no switching mid-answer");
  assert.ok($(".typing"), "dots until the first visible text");

  emit("chat-delta", ""); // still thinking
  assert.ok($(".typing"));
  emit("chat-delta", "Hel");
  emit("chat-delta", "Hello **there**");
  view.sync(); // another view update mid-stream must not wipe the live answer
  assert.equal($(".typing"), null);
  const replies = view.el.find(".reply");
  assert.equal(replies.length, 1);
  assert.equal(replies[0].find("STRONG")[0].textContent, "there");

  finish({ text: "Hello **there**!" });
  await flush();
  view.sync();
  assert.equal(view.el.find(".reply").length, 1);
  assert.equal(view.el.find(".reply")[0].textContent, "Hello there!");
  assert.deepEqual(State.chatHistory.map((m) => m.role), ["user", "assistant"]);
  assert.ok(!$(".model-btn").disabled);
});

for (const [id, name] of [["claude-code", "Claude Code"], ["codex", "Codex"]]) {
  test(`${name} chat uses no API key and can stop an outstanding reply`, async () => {
    State.settings = { ...State.settings, chatProvider: id };
    answers.chat_models = [{ id: "default", label: `${name} default` }];
    view.sync();
    assert.equal($(".model-name").textContent, `${name} · default`);
    $(".model-btn").fire("click");
    await flush();
    assert.deepEqual(sent("secret_present"), []);
    assert.deepEqual(sent("chat_models"), [{ provider: id }]);
    view.el.find(".picker-model")[0].fire("click");
    let fail;
    answers.chat_send = () => new Promise((_, reject) => { fail = reject; });
    answers.agent_chat_cancel = () => { fail(new Error("Reply stopped.")); };
    $(".chat-input").value = "hello";
    $(".send-btn").fire("click");
    await flush();
    assert.equal($(".send-btn").title, "Stop");
    assert.equal($(".send-btn").disabled, false);
    $(".send-btn").fire("click");
    await flush();
    assert.equal(sent("agent_chat_cancel").length, 1);
    assert.equal(State.chatHistory.filter((m) => m.role === "assistant").length, 0);
    assert.equal(State.noteMessage, "Reply stopped.");
    assert.equal($(".model-btn").disabled, false);
  });
}

test("microphone input is editable and never sends itself", async () => {
  $(".chat-input").value = "Please summarize";
  $(".chat-mic").fire("click");
  await flush();
  assert.equal(sent("chat_dictation_start").length, 1);
  $(".send-btn").fire("click");
  assert.equal(sent("chat_send").length, 0);
  emit("chat-dictation", { phase: "recording", level: 0.3, device: "USB microphone", text: null, error: null });
  assert.equal($(".chat-mic-level").value, 0.3);
  $(".chat-mic").fire("click");
  await flush();
  assert.deepEqual(sent("chat_dictation_stop").at(-1), { cancel: false });
  emit("chat-dictation", { phase: "idle", level: 0, device: "USB microphone", text: "the meeting", error: null });
  assert.equal($(".chat-input").value, "Please summarize the meeting");
  assert.equal(sent("chat_send").length, 0);
});

test("leaving chat cancels microphone capture and ignores a late transcript", async () => {
  $(".chat-input").value = "draft";
  $(".chat-mic").fire("click");
  await flush();
  State.view = "overview";
  view.sync();
  await flush();
  assert.deepEqual(sent("chat_dictation_stop").at(-1), { cancel: true });
  emit("chat-dictation", { phase: "idle", level: 0, device: "Mic", text: "late", error: null });
  assert.equal($(".chat-input").value, "draft");
});

test("a selected window title starts a fresh context and waits for Send", async () => {
  answers.chat_windows = [{ id: "chosen-window", appName: "Code", title: "notes.md" }];
  answers.chat_window_context = { appName: "Code", title: "notes.md" };
  $(".chat-window").fire("click");
  await flush();
  const list = $(".chat-window-list");
  list.value = "0";
  list.onchange();
  await flush();
  assert.deepEqual(State.promptContext, { kind: "window", appName: "Code", title: "notes.md" });
  assert.equal(sent("chat_send").length, 0);
  answers.chat_send = { text: "OK" };
  $(".chat-input").value = "Help with this";
  $(".send-btn").fire("click");
  await flush();
  assert.deepEqual(sent("chat_send")[0].context, State.promptContext);
});

test("browser addresses are requested only for the chosen window and previewed before Send", async () => {
  answers.chat_windows = [{ id: "browser-choice", appName: "Edge", title: "Docs" }];
  answers.chat_window_context = { appName: "Edge", title: "Docs", url: "https://example.com/docs" };
  $(".chat-window").fire("click");
  await flush();
  assert.equal(sent("chat_window_context").length, 0);
  const list = $(".chat-window-list");
  list.value = "0";
  list.onchange();
  await flush();
  view.sync();
  assert.deepEqual(sent("chat_window_context"), [{ id: "browser-choice" }]);
  assert.match(view.el.textContent, /https:\/\/example.com\/docs/);
  assert.equal(sent("chat_send").length, 0);
  $(".chat-input").value = "Explain this page";
  $(".send-btn").fire("click");
  await flush();
  assert.equal(sent("chat_send")[0].context.url, "https://example.com/docs");
});

test("leaving chat ignores a delayed browser attachment", async () => {
  let reply;
  answers.chat_windows = [{ id: "browser-choice", appName: "Edge", title: "Docs" }];
  answers.chat_window_context = () => new Promise(resolve => { reply = resolve; });
  $(".chat-window").fire("click");
  await flush();
  const list = $(".chat-window-list");
  list.value = "0";
  list.onchange();
  await flush();
  State.view = "overview";
  view.sync();
  reply({ appName: "Edge", title: "Docs", url: "https://example.com/docs" });
  await flush();
  assert.equal(State.promptContext, null);
  assert.equal(sent("chat_send").length, 0);
});

function paste(files) {
  const event = { clipboardData: { files }, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
  for (const listener of $(".chat-input").listeners.get("paste") ?? []) listener(event);
  return event;
}
const png = () => new Blob([new Uint8Array([137, 80, 78, 71])], { type: "image/png" });

test("a pasted image is previewed, removable and waits for an explicit Send", async () => {
  answers.chat_image_attach = { name: "Capture.png", path: "C:/inbox/Capture.png", size: 4 };
  assert.equal(paste([png()]).defaultPrevented, true);
  await flush();
  await flush();
  view.sync();
  assert.equal(sent("chat_image_attach").length, 1);
  assert.deepEqual(State.droppedFile, answers.chat_image_attach);
  assert.match($(".chat-image-preview").src, /^data:image\/png;base64,/);
  assert.equal($(".chat-image").hidden, false);
  assert.equal(sent("chat_send").length, 0);
  $(".chat-image-remove").fire("click");
  assert.equal(State.droppedFile, null);
  assert.equal($(".chat-image").hidden, true);
});

test("pasted screenshots become file context for either CLI only on Send", async () => {
  for (const provider of ["claude-code", "codex"]) {
    State.settings.chatProvider = provider;
    answers.chat_image_attach = { name: "Capture.png", path: "C:/inbox/Capture.png", size: 4 };
    answers.chat_send = { text: "Blue" };
    paste([png()]);
    await flush();
    await flush();
    view.sync();
    const count = sent("chat_send").length;
    $(".chat-input").value = "Describe this";
    $(".send-btn").fire("click");
    await flush();
    assert.equal(sent("chat_send").length, count + 1);
    assert.deepEqual(sent("chat_send").at(-1).context, { kind: "file", name: "Capture.png", path: "C:/inbox/Capture.png" });
    view.sync();
  }
});

test("plain text paste is untouched and oversized or multiple images are rejected", async () => {
  assert.equal(paste([]).defaultPrevented, false);
  paste([png(), png()]);
  await flush();
  assert.equal(sent("chat_image_attach").length, 0);
  paste([{ type: "image/png", size: 10 * 1024 * 1024 }]);
  await flush();
  assert.equal(sent("chat_image_attach").length, 0);
  assert.match($(".chat-image-status").textContent, /too large/);
});

test("leaving chat discards an image that finishes attaching later", async () => {
  let finish;
  answers.chat_image_attach = () => new Promise(resolve => { finish = resolve; });
  paste([png()]);
  await flush();
  State.view = "overview";
  view.sync();
  finish({ name: "Capture.png", path: "C:/inbox/Capture.png", size: 4 });
  await flush();
  assert.equal(State.droppedFile, null);
  assert.equal($(".chat-image").hidden, true);
  assert.equal(sent("chat_send").length, 0);
});

test("once the answer is in, the field takes the focus back, enabled", async () => {
  let finish;
  answers.chat_send = () => new Promise((resolve) => (finish = resolve));
  const input = $(".chat-input");
  const focused = [];
  input.focus = () => focused.push(input.disabled ? "disabled" : "enabled");
  input.value = "hello";
  $(".send-btn").fire("click");
  await flush();
  view.sync(); // mid-answer: the field is disabled
  assert.ok(input.disabled);

  finish({ text: "Hi!" });
  await flush(); // no sync yet: in the island it runs on the next frame
  assert.equal(input.disabled, false);
  assert.deepEqual(focused, ["enabled"]);
});

test("a failed answer leaves the chat for the note without taking the focus", async () => {
  answers.chat_send = () => Promise.reject(new Error("no key"));
  const input = $(".chat-input");
  const focused = [];
  input.focus = () => focused.push(true);
  input.value = "hello";
  $(".send-btn").fire("click");
  await flush();
  await flush();
  assert.equal(State.view, "note");
  assert.deepEqual(focused, []);
});
