import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { calls, internals, sent } from "./tauri.mjs";
installFakeDom();
Object.defineProperty(globalThis, "sessionStorage", { configurable: true, value: { getItem: () => "", setItem: () => {} } });
const { connectionsSection } = await import("../src/settings/connections.ts");
let statuses, opened, selected, flowFailure;
const invoke = internals.invoke;
internals.invoke = async (command, args) => {
  const value = await invoke(command, args);
  if (command === "agent_connections") return statuses;
  if (command === "agent_open" && opened) return opened(args);
  if (command === "flow_use_agent" && flowFailure) throw new Error(flowFailure);
  return value;
};
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
beforeEach(() => {
  calls.length = 0;
  selected = "anthropic";
  opened = null;
  flowFailure = null;
  statuses = [["claude-code", "Claude Code"], ["codex", "Codex"]].map(([id, name]) => ({ id, name, installed: true, signedIn: true, message: "Connected with your existing sign-in." }));
});
const build = () => {
  const section = connectionsSection(() => selected, async (id) => { selected = id; });
  // The minimal fake DOM stores attributes without HTML property reflection.
  const input = section.find("INPUT")[0];
  input.value = input.getAttribute("value") ?? "";
  return section;
};
const rowFor = (section, name) => section.find(".row").find((row) => row.children[0]?.textContent === name);
const button = (row, text) => row.find("BUTTON").find((b) => b.textContent === text);
for (const [id, name] of [["claude-code", "Claude Code"], ["codex", "Codex"]]) {
  test(`${name}: one explicit action selects chat and all Flow text providers`, async () => {
    const section = build();
    await flush();
    assert.equal(sent("flow_use_agent").length, 0);
    button(rowFor(section, name), "Use for all text").fire("click");
    await flush();
    assert.equal(selected, id);
    assert.deepEqual(sent("flow_use_agent"), [{ agent: id }]);
    assert.equal(sent("flow_set_prefs").length, 0, "must not replace audio, hotkey or feature switches from an old snapshot");
    assert.equal(sent("secret_set").length, 0);
    assert.match(section.textContent, /selected for all text features/);
  });
  test(`${name}: a failed Flow save never claims all providers were configured`, async () => {
    flowFailure = "disk is full";
    const section = build();
    await flush();
    button(rowFor(section, name), "Use for all text").fire("click");
    await flush();
    assert.match(section.textContent, /Chat is selected, but Flow settings could not be saved.*disk is full/);
    assert.doesNotMatch(section.textContent, /selected for all text features/);
    assert.equal(selected, id);
  });
  test(`${name}: an existing sign-in selects chat and opens the exact project`, async () => {
    const section = build();
    await flush();
    let row = rowFor(section, name);
    assert.equal(button(row, "Sign in"), undefined);
    button(row, "Use for chat").fire("click");
    await flush();
    assert.equal(selected, id);
    assert.equal(sent("secret_set").length, 0);
    section.find("INPUT")[0].value = "C:\\work\\my project";
    row = rowFor(section, name);
    button(row, "Open project").fire("click");
    await flush();
    assert.deepEqual(sent("agent_open"), [{ agent: id, login: false, project: "C:\\work\\my project" }]);
  });
  test(`${name}: signed-out and missing CLIs cannot start chat or a project`, async () => {
    const status = statuses.find((s) => s.id === id);
    status.signedIn = false;
    let section = build();
    await flush();
    let row = rowFor(section, name);
    assert.equal(button(row, "Use for chat").getAttribute("disabled"), "");
    assert.equal(button(row, "Use for all text").getAttribute("disabled"), "");
    assert.equal(button(row, "Open project").getAttribute("disabled"), "");
    button(row, "Sign in").fire("click");
    await flush();
    assert.deepEqual(sent("agent_open"), [{ agent: id, login: true, project: null }]);
    status.installed = false;
    section = build();
    await flush();
    row = rowFor(section, name);
    assert.equal(button(row, "Sign in").getAttribute("disabled"), "");
  });
  test(`${name}: launch failures stay visible and leave the control usable`, async () => {
    opened = () => { throw new Error("Choose an existing, absolute project folder."); };
    const section = build();
    await flush();
    const open = button(rowFor(section, name), "Open project");
    open.fire("click");
    await flush();
    assert.match(section.textContent, /Choose an existing, absolute project folder/);
    assert.equal(open.disabled, false);
  });
}
