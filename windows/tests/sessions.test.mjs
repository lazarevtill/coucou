// Run with `npm test` (Node's built-in runner; no dependencies).
//
// The cases come from what was measured on a live machine: several Claude Code
// sessions at once, mostly in bypass mode, where a question reaches the relay as
// a PermissionRequest for the AskUserQuestion tool.

import test from "node:test";
import assert from "node:assert/strict";
import { SessionStore, stepLabel, approvalTarget, aliasProjectName, lastPathComponent } from "../src/core/sessions.ts";

const T0 = 1_000_000;
const WT = { kind: "windows-terminal", shell: "powershell", label: "Windows Terminal", shellLabel: "PowerShell" };
const CURSOR = { kind: "cursor", shell: "powershell", label: "Cursor", shellLabel: "PowerShell" };

/** One hook event for a session. */
const ev = (id, name, extra = {}) => ({
  session_id: id,
  hook_event_name: name,
  cwd: `C:\\Users\\u\\${id}`,
  ...extra,
});

const ASK = {
  tool_name: "AskUserQuestion",
  tool_input: {
    questions: [{ question: "Which environment?", header: "Env", multiSelect: false, options: [] }],
  },
};

// ── Parallel sessions: the bug this exists to fix ─────────────────────────────

test("one session finishing does not mark its neighbour finished", () => {
  const s = new SessionStore();
  s.apply(ev("a", "UserPromptSubmit", { prompt: "fix it" }), T0);
  s.apply(ev("b", "UserPromptSubmit", { prompt: "write docs" }), T0 + 1);
  s.apply(ev("b", "Stop", { last_assistant_message: "done" }), T0 + 2);

  assert.equal(s.get("a").state, "thinking");
  assert.equal(s.get("b").state, "finished");
  assert.equal(s.summary(T0 + 3).state, "thinking", "the working session decides the character");
  assert.equal(s.summary(T0 + 3).count, 2);
});

test("SessionEnd removes only that session", () => {
  const s = new SessionStore();
  s.apply(ev("a", "SessionStart"), T0);
  s.apply(ev("b", "SessionStart"), T0);
  const change = s.apply(ev("b", "SessionEnd"), T0 + 1);
  assert.equal(change.removed, true);
  assert.equal(s.get("b"), undefined);
  assert.equal(s.size, 1);
  assert.ok(s.get("a"));
});

test("SessionEnd for a session never seen is harmless", () => {
  const s = new SessionStore();
  const change = s.apply(ev("ghost", "SessionEnd"), T0);
  assert.equal(change.removed, false);
  assert.equal(s.size, 0);
});

test("events from a session Coucou started in the middle of create it", () => {
  const s = new SessionStore();
  s.apply(ev("late", "PreToolUse", { tool_name: "Bash", tool_input: { command: "ls" } }), T0);
  assert.equal(s.get("late").state, "working");
  assert.equal(s.get("late").project, "late");
});

test("events with no session id from an older relay group by folder", () => {
  const s = new SessionStore();
  s.apply({ hook_event_name: "UserPromptSubmit", cwd: "C:\\p\\one", prompt: "x" }, T0);
  s.apply({ hook_event_name: "PreToolUse", cwd: "C:\\p\\one", tool_name: "Bash", tool_input: { command: "ls" } }, T0 + 1);
  s.apply({ hook_event_name: "UserPromptSubmit", cwd: "C:\\p\\two", prompt: "y" }, T0 + 2);
  assert.equal(s.size, 2);
});

// ── Waiting for the human ─────────────────────────────────────────────────────

test("AskUserQuestion is a question, with its text, whichever event announces it", () => {
  const s = new SessionStore();
  const change = s.apply(ev("a", "PreToolUse", ASK), T0);
  assert.equal(s.get("a").state, "waiting");
  assert.equal(s.get("a").waitingKind, "question");
  assert.equal(s.get("a").waitingFor, "Which environment?");
  assert.equal(change.became, "waiting");
});

test("a question that arrives as a PermissionRequest is not a permission card", () => {
  // Measured in bypass mode: PreToolUse, then PermissionRequest, for AskUserQuestion.
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", ASK), T0);
  const change = s.apply(ev("a", "PermissionRequest", ASK), T0 + 1);
  assert.equal(change.declineApproval, true, "the terminal must be handed the question");
  assert.equal(s.get("a").state, "waiting");
  assert.equal(s.get("a").waitingKind, "question");
  assert.equal(change.became, null, "already announced by PreToolUse — no second alert");
});

test("a real permission request is an approval with what it authorises", () => {
  const s = new SessionStore();
  const change = s.apply(ev("a", "PermissionRequest", { tool_name: "Bash", tool_input: { command: "rm -rf build" } }), T0);
  assert.equal(change.declineApproval, false);
  assert.equal(change.became, "approval");
  assert.equal(s.get("a").state, "approval");
  assert.equal(s.get("a").waitingFor, "Bash · rm -rf build");
});

test("Notification types that mean 'a human is needed' make a session wait", () => {
  for (const type of ["permission_prompt", "elicitation_dialog", "elicitation_url_dialog", "agent_needs_input"]) {
    const s = new SessionStore();
    s.apply(ev("a", "PreToolUse", { tool_name: "Bash", tool_input: { command: "ls" } }), T0);
    const change = s.apply(ev("a", "Notification", { notification_type: type, message: "Claude needs you" }), T0 + 1);
    assert.equal(s.get("a").state, "waiting", type);
    assert.equal(s.get("a").waitingFor, "Claude needs you", type);
    assert.equal(change.became, "waiting", type);
  }
});

test("idle and informational notifications change nothing", () => {
  for (const type of ["idle_prompt", "auth_success", "agent_completed", "elicitation_complete", "quota_auto_resume_fired"]) {
    const s = new SessionStore();
    s.apply(ev("a", "Stop", { last_assistant_message: "done" }), T0);
    const change = s.apply(ev("a", "Notification", { notification_type: type, message: "hello?" }), T0 + 1);
    assert.equal(s.get("a").state, "finished", `${type} must not turn a finished session into a waiting one`);
    assert.equal(change.became, null, type);
  }
});

test("an older Claude Code with no notification type is judged by a trailing question mark", () => {
  const s = new SessionStore();
  s.apply(ev("a", "Notification", { message: "Should I continue?" }), T0);
  assert.equal(s.get("a").state, "waiting");
  const s2 = new SessionStore();
  s2.apply(ev("a", "Notification", { message: "Task complete." }), T0);
  assert.notEqual(s2.get("a")?.state, "waiting");
});

test("a rate-limit notification is a rate limit", () => {
  const s = new SessionStore();
  s.apply(ev("a", "Notification", { notification_type: "quota_auto_resume_stale", message: "Rate limit reached" }), T0);
  assert.equal(s.get("a").state, "ratelimit");
});

test("a rate limit is announced once, not on every repeat", () => {
  const s = new SessionStore();
  const first = s.apply(ev("a", "Notification", { message: "Rate limit reached" }), T0);
  const again = s.apply(ev("a", "Notification", { message: "Rate limit reached" }), T0 + 1);
  assert.equal(first.became, "ratelimit");
  assert.equal(again.became, null);
});

test("a permission request the island could not take waits in the terminal instead", () => {
  // Another card was already up, or nobody answered in time: the relay handed
  // the request back and Claude Code is now asking in its terminal.
  const s = new SessionStore();
  s.apply(ev("a", "PermissionRequest", { tool_name: "Bash", tool_input: { command: "rm -rf build" } }), T0);
  s.handBack("a");
  assert.equal(s.get("a").state, "waiting");
  assert.equal(s.get("a").waitingKind, "permission");
  assert.equal(s.get("a").waitingFor, "Bash · rm -rf build", "what it waits on is still worth showing");
});

test("handing back leaves a session that is not on a card alone", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", { tool_name: "Bash", tool_input: { command: "ls" } }), T0);
  s.handBack("a");
  s.handBack("nobody");
  assert.equal(s.get("a").state, "working");
});

test("an answered card puts the session back to work", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PermissionRequest", { tool_name: "Bash", tool_input: { command: "ls" } }), T0);
  s.resolveApproval("a");
  assert.equal(s.get("a").state, "working");
  assert.equal(s.get("a").waitingFor, null);
});

test("the notification that follows a question does not turn it into a permission", () => {
  // Measured with a real session: AskUserQuestion, then PermissionRequest, then
  // Notification(permission_prompt, "Claude needs your permission…").
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", ASK), T0);
  s.apply(ev("a", "PermissionRequest", ASK), T0 + 1);
  const change = s.apply(ev("a", "Notification", { notification_type: "permission_prompt", message: "Claude needs your permission to use AskUserQuestion" }), T0 + 2);
  assert.equal(s.get("a").waitingKind, "question");
  assert.equal(s.get("a").waitingFor, "Which environment?");
  assert.equal(change.became, null, "already announced");
});

test("the notification that follows a handed-back permission keeps what it authorises", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PermissionRequest", { tool_name: "Bash", tool_input: { command: "rm -rf build" } }), T0);
  s.handBack("a");
  s.apply(ev("a", "Notification", { notification_type: "permission_prompt", message: "Claude needs your permission to use Bash" }), T0 + 1);
  assert.equal(s.get("a").waitingFor, "Bash · rm -rf build");
});

test("a notification never downgrades an open approval", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PermissionRequest", { tool_name: "Bash", tool_input: { command: "ls" } }), T0);
  s.apply(ev("a", "Notification", { notification_type: "permission_prompt", message: "Allow?" }), T0 + 1);
  assert.equal(s.get("a").state, "approval");
});

test("waiting ends when the human has answered, however that shows up", () => {
  const ends = [
    ["UserPromptSubmit", { prompt: "go on" }, "thinking"],
    ["PostToolUse", { ...ASK }, "working"],
    ["PostToolUse", { tool_name: "Bash", tool_input: { command: "ls" } }, "working"],
    ["PreToolUse", { tool_name: "Bash", tool_input: { command: "ls" } }, "working"],
    ["Stop", { last_assistant_message: "ok" }, "finished"],
  ];
  for (const [name, extra, expected] of ends) {
    const s = new SessionStore();
    s.apply(ev("a", "PreToolUse", ASK), T0);
    s.apply(ev("a", name, extra), T0 + 1);
    assert.equal(s.get("a").state, expected, name);
    assert.equal(s.get("a").waitingFor, null, `${name} must clear what it was waiting on`);
  }
});

// ── Finished, failed, and "have I looked yet" ─────────────────────────────────

test("a finished session stays unseen until looked at, and a new prompt clears it", () => {
  const s = new SessionStore();
  const change = s.apply(ev("a", "Stop", { last_assistant_message: "All done here" }), T0);
  assert.equal(change.became, "finished");
  assert.equal(s.get("a").unseen, true);
  assert.equal(s.get("a").steps.at(-1), "All done here");
  assert.equal(s.summary(T0 + 1).unseenFinished, 1);

  s.markSeen("a");
  assert.equal(s.get("a").unseen, false);
  assert.equal(s.get("a").state, "finished", "seen is not the same as gone");

  s.apply(ev("a", "Stop", { last_assistant_message: "again" }), T0 + 2);
  assert.equal(s.get("a").unseen, true);
  s.apply(ev("a", "UserPromptSubmit", { prompt: "next" }), T0 + 3);
  assert.equal(s.get("a").unseen, false);
  assert.equal(s.get("a").finishedAt, null);
});

test("markSeen with no id marks every session", () => {
  const s = new SessionStore();
  s.apply(ev("a", "Stop"), T0);
  s.apply(ev("b", "StopFailure", { error_message: "boom" }), T0);
  s.markSeen();
  assert.equal(s.get("a").unseen, false);
  assert.equal(s.get("b").unseen, false);
});

test("a late PostToolUse cannot reopen a finished session, but a new PreToolUse can", () => {
  const s = new SessionStore();
  s.apply(ev("a", "Stop"), T0);
  s.apply(ev("a", "PostToolUse", { tool_name: "Bash", tool_input: {} }), T0 + 1);
  assert.equal(s.get("a").state, "finished", "a stray PostToolUse after Stop is a race, not a new turn");
  s.apply(ev("a", "PreToolUse", { tool_name: "Bash", tool_input: { command: "ls" } }), T0 + 2);
  assert.equal(s.get("a").state, "working", "a Stop hook can ask Claude to carry on");
  assert.equal(s.get("a").unseen, false);
});

test("a StopFailure is an error, unseen, with its reason", () => {
  const s = new SessionStore();
  const change = s.apply(ev("a", "StopFailure", { error_type: "rate_limit", error_message: "Rate limit exceeded" }), T0);
  assert.equal(s.get("a").state, "error");
  assert.equal(s.get("a").unseen, true);
  assert.equal(s.get("a").steps.at(-1), "Rate limit exceeded");
  assert.equal(change.became, "error");
});

// ── What a session records ────────────────────────────────────────────────────

test("host, folder, project and permission mode are recorded and kept", () => {
  const s = new SessionStore();
  s.apply(ev("a", "SessionStart", { coucou_host: WT, permission_mode: "bypassPermissions", cwd: "C:\\src\\notch-buddy" }), T0);
  const a = s.get("a");
  assert.deepEqual(a.host, WT);
  assert.equal(a.permissionMode, "bypassPermissions");
  assert.equal(a.cwd, "C:\\src\\notch-buddy");
  assert.equal(a.project, "Notch Buddy", "the existing project alias still applies");

  // Later events that omit these keep what is known.
  s.apply({ session_id: "a", hook_event_name: "PostToolUse", tool_name: "Read", tool_input: {} }, T0 + 1);
  assert.deepEqual(s.get("a").host, WT);
  assert.equal(s.get("a").permissionMode, "bypassPermissions");
  assert.equal(s.get("a").cwd, "C:\\src\\notch-buddy");

  // And a changed mode is picked up.
  s.apply(ev("a", "UserPromptSubmit", { permission_mode: "plan", prompt: "x" }), T0 + 2);
  assert.equal(s.get("a").permissionMode, "plan");
});

test("steps keep the last twenty, labelled the way the island always has", () => {
  const s = new SessionStore();
  for (let i = 0; i < 25; i++) {
    s.apply(ev("a", "PreToolUse", { tool_name: "Read", tool_input: { file_path: `C:\\p\\f${i}.ts` } }), T0 + i);
  }
  const steps = s.get("a").steps;
  assert.equal(steps.length, 20);
  assert.equal(steps.at(-1), "Lit · f24.ts");
  assert.equal(steps[0], "Lit · f5.ts");
});

test("a prompt becomes a short first step; a failed tool is marked", () => {
  const s = new SessionStore();
  s.apply(ev("a", "UserPromptSubmit", { prompt: "x".repeat(100) }), T0);
  assert.equal(s.get("a").steps.at(-1).length, 60);
  s.apply(ev("a", "PostToolUseFailure", { tool_name: "Bash", tool_input: {} }), T0 + 1);
  assert.equal(s.get("a").steps.at(-1), "⚠ failed");
  assert.equal(s.get("a").state, "working");
});

test("subagent events add a step and leave the state alone", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", { tool_name: "Task", tool_input: {} }), T0);
  s.apply(ev("a", "SubagentStart"), T0 + 1);
  s.apply(ev("a", "SubagentStop"), T0 + 2);
  assert.equal(s.get("a").state, "working");
  assert.deepEqual(s.get("a").steps.slice(-2), ["+ subagent", "• subagent done"]);
});

// ── Ordering and the summary the pill shows ───────────────────────────────────

test("list puts what needs the human first, then failures, then work, then the finished", () => {
  const s = new SessionStore();
  s.apply(ev("done", "Stop"), T0);
  s.apply(ev("work", "PreToolUse", { tool_name: "Bash", tool_input: { command: "ls" } }), T0 + 1);
  s.apply(ev("err", "StopFailure", { error_message: "x" }), T0 + 2);
  s.apply(ev("ask", "PreToolUse", ASK), T0 + 3);
  s.apply(ev("approve", "PermissionRequest", { tool_name: "Bash", tool_input: { command: "x" } }), T0 + 4);
  assert.deepEqual(s.list().map((x) => x.id), ["approve", "ask", "err", "work", "done"]);
});

test("equally urgent sessions are ordered most recent first", () => {
  const s = new SessionStore();
  s.apply(ev("old", "PreToolUse", { tool_name: "Bash", tool_input: {} }), T0);
  s.apply(ev("new", "PreToolUse", { tool_name: "Bash", tool_input: {} }), T0 + 10);
  assert.deepEqual(s.list().map((x) => x.id), ["new", "old"]);
});

test("the summary counts, picks the top session and names the badge", () => {
  const s = new SessionStore();
  assert.deepEqual(s.summary(T0).badge, null);
  assert.equal(s.summary(T0).state, "idle");

  s.apply(ev("w", "PreToolUse", { tool_name: "Bash", tool_input: {} }), T0);
  assert.equal(s.summary(T0).badge, null, "plain work earns no badge");
  assert.equal(s.summary(T0).state, "working");

  s.apply(ev("d", "Stop"), T0 + 1);
  assert.equal(s.summary(T0 + 2).badge, "finished");
  assert.equal(s.summary(T0 + 2).unseenFinished, 1);

  s.apply(ev("e", "StopFailure", { error_message: "x" }), T0 + 3);
  assert.equal(s.summary(T0 + 4).badge, "error");

  s.apply(ev("q", "PreToolUse", ASK), T0 + 5);
  const sum = s.summary(T0 + 6);
  assert.equal(sum.badge, "needs-you");
  assert.equal(sum.waiting, 1);
  assert.equal(sum.state, "waiting");
  assert.equal(sum.top.id, "q");
  assert.equal(sum.count, 4);
  assert.equal(sum.working, 1);
});

test("a finished session celebrates for a few seconds, then the character is idle again", () => {
  const s = new SessionStore();
  s.apply(ev("a", "Stop"), T0);
  assert.equal(s.summary(T0 + 1_000).state, "finished");
  assert.equal(s.summary(T0 + 6_000).state, "idle");
  assert.equal(s.summary(T0 + 6_000).badge, "finished", "unseen still shows on the pill after the celebration");
});

test("remove drops a session the app has found dead", () => {
  const s = new SessionStore();
  s.apply(ev("a", "SessionStart"), T0);
  assert.equal(s.remove("a"), true);
  assert.equal(s.remove("a"), false);
  assert.equal(s.size, 0);
});

// ── The helpers that moved here keep behaving ─────────────────────────────────

test("step labels, approval targets and project names are unchanged", () => {
  assert.equal(stepLabel("Bash", { command: "npm test" }), "Exécute · npm test");
  assert.equal(stepLabel("Edit", { file_path: "C:\\p\\src\\a.ts" }), "Modifie · a.ts");
  assert.equal(stepLabel("Mystery", {}), "Mystery");
  assert.equal(approvalTarget("Write", { file_path: "C:\\p\\.env" }), "Write · C:\\p\\.env");
  assert.equal(approvalTarget("Mystery", {}), "Mystery");
  assert.equal(lastPathComponent("C:\\a\\b\\"), "b");
  assert.equal(aliasProjectName("NotchBuddy"), "Notch Buddy");
  assert.equal(aliasProjectName("coucou"), "coucou");
});

// ── The code view: each session's latest edit ────────────────────────────────

const EDIT_INPUT = { file_path: "C:/work/shop/invoice.ts", old_string: "const TVA = 0.196", new_string: "const TVA = 0.2", replace_all: false };
const EDIT_PATCH = { structuredPatch: [{ oldStart: 4, oldLines: 1, newStart: 4, newLines: 1, lines: ["-const TVA = 0.196", "+const TVA = 0.2"] }] };

test("an edit shows up as soon as it starts, and with line numbers once it ran", () => {
  const s = new SessionStore();
  s.apply(ev("a", "SessionStart"), T0);
  assert.equal(s.get("a").edit, null);

  s.apply(ev("a", "PreToolUse", { tool_name: "Edit", tool_input: EDIT_INPUT }), T0 + 1);
  assert.equal(s.get("a").edit.status, "pending");
  assert.equal(s.get("a").edit.numbered, false);

  s.apply(ev("a", "PostToolUse", { tool_name: "Edit", tool_input: EDIT_INPUT, tool_response: EDIT_PATCH }), T0 + 2);
  const e = s.get("a").edit;
  assert.equal(e.status, "applied");
  assert.equal(e.numbered, true);
  assert.deepEqual(e.lines.map((l) => l.num), [4, 4]);
});

test("with an older relay that sends no patch, the edit is still marked done", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", { tool_name: "Edit", tool_input: EDIT_INPUT }), T0);
  s.apply(ev("a", "PostToolUse", { tool_name: "Edit", tool_input: EDIT_INPUT }), T0 + 1);
  assert.equal(s.get("a").edit.status, "applied");
  assert.equal(s.get("a").edit.numbered, false);
});

test("a failed edit says so, and other tools leave the last edit in place", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", { tool_name: "Edit", tool_input: EDIT_INPUT }), T0);
  s.apply(ev("a", "PostToolUseFailure", { tool_name: "Edit", tool_input: EDIT_INPUT }), T0 + 1);
  assert.equal(s.get("a").edit.status, "failed");

  s.apply(ev("a", "PreToolUse", { tool_name: "Bash", tool_input: { command: "npm test" } }), T0 + 2);
  s.apply(ev("a", "PostToolUse", { tool_name: "Bash", tool_input: { command: "npm test" } }), T0 + 3);
  assert.equal(s.get("a").edit.name, "invoice.ts");
});

test("an edit waiting for permission is the one shown", () => {
  const s = new SessionStore();
  const write = { file_path: "C:/work/shop/notes.md", content: "alpha" };
  s.apply(ev("a", "PermissionRequest", { tool_name: "Write", tool_input: write }), T0);
  assert.equal(s.get("a").edit.name, "notes.md");
  assert.equal(s.get("a").edit.status, "pending");
});

test("each session keeps its own edit", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", { tool_name: "Edit", tool_input: EDIT_INPUT }), T0);
  s.apply(ev("b", "PreToolUse", { tool_name: "Write", tool_input: { file_path: "C:/x/main.rs", content: "fn main() {}" } }), T0 + 1);
  assert.equal(s.get("a").edit.name, "invoice.ts");
  assert.equal(s.get("b").edit.name, "main.rs");
});
