import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";

installFakeDom();
document.documentElement = { lang: "en" };
const { State, DEFAULT_SETTINGS } = await import("../src/core/state.ts");
const { SessionStore } = await import("../src/core/sessions.ts");
const { Bridge } = await import("../src/core/bridge.ts");
const { sessionConnections } = await import("../src/views/sessions.ts");
const { projectsSection } = await import("../src/settings/projects.ts");
const tick = () => new Promise(resolve => setImmediate(resolve));

for (const agent of ["claude", "codex"]) {
  test(`${agent}: selecting a session routes focus and project launch without changing approval`, async t => {
    State.settings = { ...DEFAULT_SETTINGS };
    State.sessions = new SessionStore();
    const approval = { requestId: "approval-to-preserve" };
    State.pendingApproval = approval;
    for (const [id, type] of [["first", agent], ["second", agent], ["other-agent", agent === "claude" ? "codex" : "claude"]]) {
      State.sessions.apply({ session_id: id, coucou_agent: type, hook_event_name: "SessionStart", cwd: `C:\\projects\\${id}` }, 1);
    }
    const calls = [];
    t.mock.method(Bridge, "focusSession", async id => { calls.push(["focus", id]); return { outcome: "flashed" }; });
    t.mock.method(Bridge, "openProject", async (target, path) => { calls.push([target, path]); return { result: { error: null } }; });
    const view = sessionConnections();
    assert.equal(view.sync(agent), true);
    assert.equal(view.el.find("option").length, 2);
    const select = view.el.find("select")[0];
    select.value = "second";
    select.fire("change");
    view.el.find("button").find(b => b.textContent === "Open").fire("click");
    await tick();
    assert.match(view.el.textContent, /taskbar/);
    view.el.find("button").find(b => b.textContent === "Open in VS Code").fire("click");
    await tick();
    view.el.find("button").find(b => b.textContent === "Open terminal").fire("click");
    await tick();
    assert.deepEqual(calls, [["focus", "second"], ["editor", "C:\\projects\\second"], ["terminal", "C:\\projects\\second"]]);
    assert.equal(State.pendingApproval, approval);
    State.sessions.apply({ session_id: "second", coucou_agent: agent, hook_event_name: "PreToolUse", tool_name: "Read" }, 2);
    view.sync(agent);
    assert.equal(view.el.find("select")[0].value, "second");
    State.sessions.remove("second");
    view.sync(agent);
    assert.equal(view.el.find("select")[0].value, "first");
  });
}

test("project preferences preserve unrelated settings and restore a failed selection", async () => {
  const settings = { ...DEFAULT_SETTINGS, language: "ru", terminal: "none" };
  let fail = false;
  const saved = [];
  const section = projectsSection(settings, async () => {
    if (fail) throw new Error("disk full");
    saved.push({ ...settings });
  });
  const [editor, terminal] = section.find("select");
  editor.value = "cursor";
  editor.fire("change");
  await tick();
  assert.equal(saved[0].editor, "cursor");
  assert.equal(saved[0].terminal, "none");
  assert.equal(saved[0].language, "ru");
  fail = true;
  terminal.value = "shell";
  terminal.fire("change");
  await tick();
  assert.equal(settings.terminal, "none");
  assert.equal(terminal.value, "none");
  assert.match(section.textContent, /disk full/);
});

test("session tracking separates agents without IDs and bounds retained sessions", () => {
  const sessions = new SessionStore();
  for (const agent of ["claude", "codex"]) sessions.apply({ coucou_agent: agent, cwd: "C:\\shared", hook_event_name: "SessionStart" }, 1);
  assert.equal(sessions.list("claude").length, 1);
  assert.equal(sessions.list("codex").length, 1);
  sessions.apply({ coucou_agent: "codex", cwd: "C:\\shared", hook_event_name: "SessionEnd" }, 2);
  assert.equal(sessions.list("claude").length, 1);
  assert.equal(sessions.list("codex").length, 0);
  for (let i = 0; i < 65; i++) sessions.apply({ session_id: `s${i}`, hook_event_name: "SessionStart" }, i + 3);
  assert.equal(sessions.size, 64);
  assert.equal(sessions.get("s0"), undefined);
  assert.ok(sessions.get("s64"));
});
