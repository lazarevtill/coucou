// The Windows shell's view of the sessions: tray icon, tooltip, flyout rows and
// toasts. Run with `npm test`.

import test from "node:test";
import assert from "node:assert/strict";
import { SessionStore } from "../src/core/sessions.ts";
import { trayState, tooltipText, headline, sessionRows, serviceRows, toastFor, staleToasts, buildSnapshot } from "../src/core/shell.ts";

const T0 = 1_000_000;
const WT = { kind: "windows-terminal", shell: "powershell", label: "Windows Terminal", shellLabel: "Windows PowerShell" };
const CURSOR = { kind: "cursor", shell: "powershell", label: "Cursor", shellLabel: "Windows PowerShell" };
const ASK = { tool_name: "AskUserQuestion", tool_input: { questions: [{ question: "Which environment?" }] } };
const BASH = { tool_name: "Bash", tool_input: { command: "npm test" } };
const ev = (id, name, extra = {}) => ({ session_id: id, hook_event_name: name, cwd: `C:\\src\\${id}`, ...extra });

// ── Tray icon ─────────────────────────────────────────────────────────────────

test("the tray icon says the most important thing", () => {
  const s = new SessionStore();
  assert.equal(trayState(s.summary(T0), false), "idle");
  s.apply(ev("d", "Stop"), T0);
  assert.equal(trayState(s.summary(T0), false), "done");
  s.apply(ev("w", "PreToolUse", BASH), T0);
  assert.equal(trayState(s.summary(T0), false), "working", "work beats a finish waiting to be seen");
  s.apply(ev("e", "StopFailure", { error_message: "x" }), T0);
  assert.equal(trayState(s.summary(T0), false), "error", "a failure beats work");
  s.apply(ev("q", "PreToolUse", ASK), T0);
  assert.equal(trayState(s.summary(T0), false), "attention", "a human being needed beats everything");
  assert.equal(trayState(s.summary(T0), true), "paused", "except being paused");
});

test("the tooltip names who needs you and where", () => {
  const s = new SessionStore();
  assert.equal(tooltipText(s.summary(T0), false), "Coucou — no Claude Code sessions");
  assert.equal(tooltipText(s.summary(T0), true), "Coucou — paused");
  s.apply(ev("api", "PreToolUse", { ...BASH, coucou_host: WT }), T0);
  assert.equal(tooltipText(s.summary(T0), false), "Coucou — 1 session · 1 working");
  s.apply(ev("web", "PreToolUse", { ...ASK, coucou_host: CURSOR }), T0 + 1);
  assert.equal(tooltipText(s.summary(T0 + 1), false), "Coucou — 2 sessions · web needs you in Cursor · 1 working");
});

test("the flyout headline counts sessions and who needs you", () => {
  const s = new SessionStore();
  assert.equal(headline(s.summary(T0)), "No Claude Code sessions");
  s.apply(ev("a", "PreToolUse", BASH), T0);
  assert.equal(headline(s.summary(T0)), "1 session · working");
  s.apply(ev("b", "PreToolUse", ASK), T0);
  s.apply(ev("c", "Stop"), T0);
  assert.equal(headline(s.summary(T0)), "3 sessions · 1 needs you");
});

// ── Flyout rows ───────────────────────────────────────────────────────────────

test("each session row says what it is doing, where, and how to get there", () => {
  const s = new SessionStore();
  s.apply(ev("api", "PreToolUse", { ...BASH, coucou_host: WT, permission_mode: "bypassPermissions" }), T0);
  s.apply(ev("web", "PreToolUse", { ...ASK, coucou_host: CURSOR }), T0 + 1);
  s.apply(ev("docs", "Stop", { last_assistant_message: "Docs updated." }), T0 + 2);
  const rows = sessionRows(s.list());
  assert.deepEqual(rows.map((r) => r.id), ["web", "api", "docs"]);
  const [web, api, docs] = rows;
  assert.equal(web.label, "Asks you");
  assert.equal(web.detail, "Which environment?");
  assert.equal(web.host, "Cursor");
  assert.equal(web.jumpLabel, "Go to Cursor");
  assert.equal(web.attention, true);
  assert.equal(api.label, "Working");
  assert.equal(api.detail, "Exécute · npm test");
  assert.equal(api.bypass, true);
  assert.equal(api.shell, "Windows PowerShell");
  assert.equal(docs.label, "Finished");
  assert.equal(docs.detail, "Docs updated.");
  assert.equal(docs.unseen, true);
  assert.equal(docs.dim, false, "an unseen finish is not dimmed");
  s.markSeen("docs");
  assert.equal(sessionRows(s.list()).find((r) => r.id === "docs").dim, true, "a seen finish is");
});

test("row labels cover every state", () => {
  const s = new SessionStore();
  s.apply(ev("perm", "PermissionRequest", BASH), T0);
  s.apply(ev("back", "PermissionRequest", BASH), T0);
  s.handBack("back");
  s.apply(ev("input", "Notification", { notification_type: "elicitation_dialog", message: "Pick a file" }), T0);
  s.apply(ev("think", "UserPromptSubmit", { prompt: "hello" }), T0);
  s.apply(ev("fail", "StopFailure", { error_message: "boom" }), T0);
  s.apply(ev("rate", "Notification", { message: "Rate limit reached" }), T0);
  s.apply(ev("idle", "SessionStart"), T0);
  const label = Object.fromEntries(sessionRows(s.list()).map((r) => [r.id, r.label]));
  assert.deepEqual(label, {
    perm: "Needs permission", back: "Needs permission", input: "Needs you",
    think: "Thinking", fail: "Failed", rate: "Rate limited", idle: "Idle",
  });
});

test("a session with no known host cannot be jumped to", () => {
  const s = new SessionStore();
  s.apply(ev("x", "PreToolUse", BASH), T0);
  const [row] = sessionRows(s.list());
  assert.equal(row.host, null);
  assert.equal(row.jumpLabel, "Go to session");
});

// ── Services ──────────────────────────────────────────────────────────────────

test("service rows say whether each integration works", () => {
  const tasks = [
    { id: "integration_claude", name: "Claude Code", color: "#fff", isIntegration: true },
    { id: "integration_vercel", name: "Vercel", color: "#7C5CFF", isIntegration: true },
    { id: "integration_github", name: "GitHub", color: "#F4505E", isIntegration: true },
    { id: "integration_n8n", name: "n8n", color: "#F29B38", isIntegration: true },
    { id: "agent_codex", name: "codex", color: "#22C55E", isIntegration: false },
  ];
  const info = {
    integration_vercel: { configured: true, loaded: true, error: null, data: {} },
    integration_github: { configured: true, loaded: true, error: "HTTP 401", data: {} },
  };
  assert.deepEqual(serviceRows(tasks, info), [
    { id: "integration_vercel", name: "Vercel", color: "#7C5CFF", status: "ok", detail: "Connected" },
    { id: "integration_github", name: "GitHub", color: "#F4505E", status: "error", detail: "HTTP 401" },
    { id: "integration_n8n", name: "n8n", color: "#F29B38", status: "off", detail: "Not set up" },
  ]);
});

// ── The snapshot the flyout draws ─────────────────────────────────────────────

test("the snapshot carries the permission card with the session it belongs to", () => {
  const s = new SessionStore();
  s.apply(ev("api", "PermissionRequest", { ...BASH, coucou_host: WT }), T0);
  s.apply(ev("web", "PreToolUse", { ...BASH, coucou_host: CURSOR }), T0);
  const snap = buildSnapshot({
    store: s, now: T0, paused: false, island: "off",
    pending: { requestId: "r1", sessionId: "api", tool: "Bash", command: "Bash · npm test" },
    tasks: [], integrations: {},
  });
  assert.deepEqual(snap.approval, { requestId: "r1", sessionId: "api", project: "api", host: "Windows Terminal", target: "Bash · npm test" });
  assert.equal(snap.trayState, "attention");
  assert.equal(snap.botState, "approval");
  assert.equal(snap.island, "off");
  assert.equal(snap.headline, "2 sessions · 1 needs you");
  assert.deepEqual(snap.sessions.map((r) => r.id), ["api", "web"]);
  assert.match(snap.tooltip, /^Coucou — 2 sessions/);
});

test("a permission card for a session nobody knows still shows what it authorises", () => {
  const snap = buildSnapshot({
    store: new SessionStore(), now: T0, paused: true, island: "on",
    pending: { requestId: "r2", sessionId: "", tool: "Write", command: "Write · C:\\x" },
    tasks: [], integrations: {},
  });
  assert.deepEqual(snap.approval, { requestId: "r2", sessionId: "", project: "Claude Code", host: null, target: "Write · C:\\x" });
  assert.equal(snap.trayState, "paused");
  assert.equal(snap.approval.target, "Write · C:\\x");
});

// ── Toasts ────────────────────────────────────────────────────────────────────

const NEEDS_YOU = { notify: "needsYou", paused: false, islandAlerted: false };

test("a question becomes a toast that goes to the session", () => {
  const s = new SessionStore();
  s.apply(ev("web", "PreToolUse", { ...ASK, coucou_host: CURSOR }), T0);
  assert.deepEqual(toastFor("waiting", s.get("web"), NEEDS_YOU), {
    tag: "web", title: "web asks in Cursor", body: "Which environment?", jump: "web", jumpLabel: "Go to Cursor",
  });
});

test("a failure is a toast; a finish only when asked for", () => {
  const s = new SessionStore();
  s.apply(ev("a", "StopFailure", { error_message: "Context window full", coucou_host: WT }), T0);
  s.apply(ev("b", "Stop", { last_assistant_message: "All tests pass.", coucou_host: WT }), T0);
  const failed = toastFor("error", s.get("a"), NEEDS_YOU);
  assert.equal(failed.title, "a stopped on an error");
  assert.equal(failed.body, "Context window full");
  assert.equal(toastFor("finished", s.get("b"), NEEDS_YOU), null);
  const done = toastFor("finished", s.get("b"), { ...NEEDS_YOU, notify: "all" });
  assert.equal(done.title, "b finished");
  assert.equal(done.body, "All tests pass.");
});

test("no toast when switched off, paused, already on the island, or for a permission card", () => {
  const s = new SessionStore();
  s.apply(ev("q", "PreToolUse", ASK), T0);
  const q = s.get("q");
  assert.equal(toastFor("waiting", q, { ...NEEDS_YOU, notify: "off" }), null);
  assert.equal(toastFor("waiting", q, { ...NEEDS_YOU, paused: true }), null);
  assert.equal(toastFor("waiting", q, { ...NEEDS_YOU, islandAlerted: true }), null);
  assert.equal(toastFor("approval", q, NEEDS_YOU), null, "an approval is answered in a window, never from a toast");
  assert.equal(toastFor("ratelimit", q, NEEDS_YOU), null);
});

test("a toast is taken back once its session no longer needs anyone", () => {
  const s = new SessionStore();
  s.apply(ev("q", "PreToolUse", ASK), T0);
  s.apply(ev("e", "StopFailure", { error_message: "x" }), T0);
  const shown = new Map([["q", "waiting"], ["e", "error"], ["gone", "waiting"]]);
  assert.deepEqual(staleToasts(shown, s.list()).sort(), ["gone"]);
  s.apply(ev("q", "PostToolUse", ASK), T0 + 1);
  s.markSeen("e");
  assert.deepEqual(staleToasts(shown, s.list()).sort(), ["e", "gone", "q"]);
});
