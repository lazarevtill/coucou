import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { internals, calls, sent } from "./tauri.mjs";
installFakeDom();
const { mcpSection, mcpSetup } = await import("../src/settings/mcp.ts");
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
const original = internals.invoke;
let status, read, save, copied, copyError;
internals.invoke = async (command, args) => {
  await original(command, args);
  if (command === "mcp_status") return read ? read() : { ...status };
  if (command === "mcp_set_enabled") {
    if (save) return save(args);
    status = { ...status, enabled: args.enabled };
    return { ...status };
  }
  return null;
};
beforeEach(() => {
  calls.length = 0;
  status = { enabled: false, windows: true, executable: "C:\\Program Files\\Coucou\\coucou.exe" };
  read = save = copyError = null;
  copied = [];
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async (text) => {
    if (copyError) throw new Error(copyError);
    copied.push(text);
  } } });
});
const permission = (section) => section.find("BUTTON").find((button) => button.getAttribute("role") === "switch");
const button = (section, text) => section.find("BUTTON").find((button) => button.textContent === text);

test("MCP settings open without granting access or changing agent configuration", async () => {
  const section = mcpSection();
  assert.equal(permission(section).disabled, true);
  await flush();
  assert.equal(permission(section).getAttribute("aria-checked"), "false");
  assert.equal(permission(section).disabled, false);
  assert.deepEqual(calls.map(([command]) => command), ["mcp_status"]);
  assert.match(section.textContent, /all saved calls/);
  assert.match(section.textContent, /AI provider/);
  assert.match(section.textContent, /cannot erase text already shared/);
});

test("permission only changes after a successful save, with duplicate clicks blocked", async () => {
  const section = mcpSection();
  await flush();
  let complete;
  save = ({ enabled }) => new Promise((resolve) => { complete = () => resolve({ ...status, enabled }); });
  const toggle = permission(section);
  toggle.fire("click");
  toggle.fire("click");
  await flush();
  assert.equal(toggle.disabled, true);
  assert.equal(toggle.getAttribute("aria-checked"), "false");
  assert.deepEqual(sent("mcp_set_enabled"), [{ enabled: true }]);
  complete();
  await flush();
  assert.equal(toggle.getAttribute("aria-checked"), "true");
  assert.equal(toggle.disabled, false);
  save = null;
  toggle.fire("click");
  await flush();
  assert.deepEqual(sent("mcp_set_enabled"), [{ enabled: true }, { enabled: false }]);
  assert.equal(toggle.getAttribute("aria-checked"), "false");
});

test("a failed permission save keeps the observed state, shows the error and allows retry", async () => {
  const section = mcpSection();
  await flush();
  save = () => { throw new Error("Disk is full"); };
  permission(section).fire("click");
  await flush();
  assert.equal(permission(section).getAttribute("aria-checked"), "false");
  assert.equal(permission(section).disabled, false);
  assert.match(section.textContent, /Disk is full/);
  save = null;
  permission(section).fire("click");
  await flush();
  assert.equal(permission(section).getAttribute("aria-checked"), "true");
  assert.doesNotMatch(section.textContent, /Disk is full/);
});

test("unknown access state stays disabled and can be refreshed", async () => {
  read = () => { throw new Error("Access setting cannot be read"); };
  const section = mcpSection();
  await flush();
  assert.equal(permission(section).disabled, true);
  assert.equal(button(section, "Copy").disabled, true);
  assert.equal(section.find("TEXTAREA")[0].value, "");
  assert.match(section.textContent, /cannot be read/);
  read = null;
  button(section, "Refresh").fire("click");
  await flush();
  assert.equal(permission(section).disabled, false);
});

test("Claude Code, Codex and Claude Desktop have equally usable setup with no client writes", async () => {
  const section = mcpSection();
  await flush();
  const choice = section.find("SELECT")[0];
  const setup = mcpSetup(status);
  for (const client of ["claude", "codex", "desktop"]) {
    choice.value = client;
    choice.fire("change");
    assert.equal(section.find("TEXTAREA")[0].value, setup[client]);
    button(section, "Copy").fire("click");
    await flush();
    assert.equal(copied.at(-1), setup[client]);
  }
  choice.value = "codex-desktop";
  choice.fire("change");
  assert.equal(section.find("TEXTAREA")[0].value, setup.codex);
  assert.ok(choice.find("OPTION").some((option) => option.textContent === "Codex desktop / ChatGPT desktop"));
  assert.match(section.textContent, /ChatGPT desktop uses the Codex configuration/);
  assert.deepEqual(calls.map(([command]) => command), ["mcp_status"]);
  assert.deepEqual(JSON.parse(setup.desktop).mcpServers["coucou-notes"], { command: status.executable, args: ["mcp"] });
});

test("MCP and desktop connection settings are discoverable from Hub search", async () => {
  const { SETTINGS_ENTRIES } = await import("../src/hub/search.ts");
  const entry = SETTINGS_ENTRIES.find((entry) => entry.id === "integrations.mcp");
  assert.equal(entry.page, "integrations");
  assert.match(entry.title, /MCP/);
  assert.ok(entry.terms.includes("Claude Desktop"));
  assert.ok(entry.terms.some((term) => term.includes("Codex desktop")));
});

test("copy failure leaves the complete selectable configuration", async () => {
  const section = mcpSection();
  await flush();
  const text = section.find("TEXTAREA")[0].value;
  copyError = "clipboard unavailable";
  button(section, "Copy").fire("click");
  await flush();
  assert.match(section.textContent, /copy the setup text manually/);
  assert.equal(section.find("TEXTAREA")[0].value, text);
});

test("setup quotes spaces, apostrophes and shell syntax without interpreting the executable path", () => {
  const path = "C:\\O'Brien\\$test; & coucou.exe";
  const windows = mcpSetup({ ...status, executable: path });
  assert.ok(windows.claude.endsWith("'C:\\O''Brien\\$test; & coucou.exe' mcp"));
  assert.ok(windows.codex.endsWith("'C:\\O''Brien\\$test; & coucou.exe' mcp"));
  assert.equal(JSON.parse(windows.desktop).mcpServers["coucou-notes"].command, path);
  const linux = mcpSetup({ ...status, executable: "/home/O'Brien/coucou", windows: false });
  assert.ok(linux.codex.endsWith("'/home/O'\"'\"'Brien/coucou' mcp"));
});
