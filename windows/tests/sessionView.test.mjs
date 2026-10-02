// What the island shows and does for the Claude Code sessions. Run with `npm test`.

import test from "node:test";
import assert from "node:assert/strict";
import { SessionStore } from "../src/core/sessions.ts";
import { botState, claudePill, reactTo, waitingSession, finishedSession, waitingLine, jumpLabel, jumpNote, staleCard } from "../src/core/sessionView.ts";

const T0 = 1_000_000;
const CURSOR = { kind: "cursor", shell: "powershell", label: "Cursor", shellLabel: "PowerShell" };
const WT = { kind: "windows-terminal", shell: "powershell", label: "Windows Terminal", shellLabel: "PowerShell" };
const ASK = {
  tool_name: "AskUserQuestion",
  tool_input: { questions: [{ question: "Which environment?", options: [] }] },
};
const ev = (id, name, extra = {}) => ({ session_id: id, hook_event_name: name, cwd: `C:\\src\\${id}`, ...extra });
const BASH = { tool_name: "Bash", tool_input: { command: "ls" } };

// ── The Claude pill ───────────────────────────────────────────────────────────

test("a question shows as the question face; every other state is its own", () => {
  assert.equal(botState("waiting"), "question");
  for (const s of ["idle", "thinking", "working", "approval", "finished", "error", "ratelimit"]) {
    assert.equal(botState(s), s);
  }
});

test("with no session the pill is plain Claude Code", () => {
  const pill = claudePill(new SessionStore().summary(T0));
  assert.deepEqual(pill, {
    name: "Claude Code", state: "idle", steps: [], cwd: null, badge: null,
    pillLabel: "Claude Code", toolLabel: "Claude Code",
  });
});

test("one session: its project, its steps, its folder and where it runs", () => {
  const s = new SessionStore();
  s.apply(ev("coucou", "PreToolUse", { ...BASH, coucou_host: CURSOR }), T0);
  const pill = claudePill(s.summary(T0));
  assert.equal(pill.name, "coucou");
  assert.equal(pill.state, "working");
  assert.deepEqual(pill.steps, ["Exécute · ls"]);
  assert.equal(pill.cwd, "C:\\src\\coucou");
  assert.equal(pill.pillLabel, "Claude Code");
  assert.equal(pill.toolLabel, "Claude Code · Cursor");
});

test("several sessions: the pill counts them and the most urgent one leads", () => {
  const s = new SessionStore();
  s.apply(ev("api", "PreToolUse", { ...BASH, coucou_host: WT }), T0);
  s.apply(ev("web", "PreToolUse", { ...BASH, coucou_host: CURSOR }), T0 + 1);
  s.apply(ev("docs", "PreToolUse", { ...ASK, coucou_host: WT }), T0 + 2);
  const pill = claudePill(s.summary(T0 + 3));
  assert.equal(pill.pillLabel, "Claude Code · 3");
  assert.equal(pill.name, "docs");
  assert.equal(pill.state, "question");
  assert.equal(pill.toolLabel, "Claude Code · Windows Terminal · 3 sessions");
});

test("a session in bypass mode says so", () => {
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", { ...BASH, coucou_host: WT, permission_mode: "bypassPermissions" }), T0);
  assert.equal(claudePill(s.summary(T0)).toolLabel, "Claude Code · Windows Terminal · bypass");
});

test("the badge says what is waiting: the human first, then failures, then finishes", () => {
  const s = new SessionStore();
  s.apply(ev("d", "Stop"), T0);
  assert.equal(claudePill(s.summary(T0)).badge, "finished");
  s.apply(ev("e", "StopFailure", { error_message: "x" }), T0);
  assert.equal(claudePill(s.summary(T0)).badge, "error");
  s.apply(ev("q", "PreToolUse", ASK), T0);
  assert.equal(claudePill(s.summary(T0)).badge, "approval");
  s.markSeen();
  s.remove("q");
  assert.equal(claudePill(s.summary(T0)).badge, null, "seen finishes and failures earn no badge");
});

// ── What the island does when something happens ───────────────────────────────

const ON = { focused: true, view: "overview" };
const OFF = { focused: false, view: "overview" };

function changeFor(store, event, now = T0) {
  return store.apply(event, now);
}

test("a question opens the island on it, with its sound", () => {
  const c = changeFor(new SessionStore(), ev("a", "PreToolUse", ASK));
  assert.deepEqual(reactTo(c, "PreToolUse", ON), { sound: "question", alert: "question", reveal: false });
});

test("a question while another pill is in front shows the island and the badge, not the card", () => {
  const c = changeFor(new SessionStore(), ev("a", "PreToolUse", ASK));
  assert.deepEqual(reactTo(c, "PreToolUse", OFF), { sound: "question", alert: null, reveal: true });
});

test("a finish or a failure opens its card when Claude is in front", () => {
  const done = changeFor(new SessionStore(), ev("a", "Stop"));
  assert.deepEqual(reactTo(done, "Stop", ON), { sound: "finish", alert: "finished", reveal: false });
  const failed = changeFor(new SessionStore(), ev("a", "StopFailure", { error_message: "x" }));
  assert.deepEqual(reactTo(failed, "StopFailure", ON), { sound: "error", alert: "error", reveal: false });
});

test("a finish or a failure elsewhere is only a sound and the badge", () => {
  const done = changeFor(new SessionStore(), ev("a", "Stop"));
  assert.deepEqual(reactTo(done, "Stop", OFF), { sound: "finish", alert: null, reveal: false });
});

test("nothing takes the island away from a card that waits on the human", () => {
  for (const view of ["approval", "question"]) {
    const ctx = { focused: true, view };
    const done = changeFor(new SessionStore(), ev("b", "Stop"));
    assert.deepEqual(reactTo(done, "Stop", ctx), { sound: "finish", alert: null, reveal: false }, view);
  }
  // A second question while the approval card is up: heard, not shown over it.
  const asked = changeFor(new SessionStore(), ev("b", "PreToolUse", ASK));
  assert.deepEqual(reactTo(asked, "PreToolUse", { focused: true, view: "approval" }), { sound: "question", alert: null, reveal: true });
});

test("a new question replaces an older one on the question card", () => {
  const asked = changeFor(new SessionStore(), ev("b", "PreToolUse", ASK));
  assert.equal(reactTo(asked, "PreToolUse", { focused: true, view: "question" }).alert, "question");
});

test("a rate limit is a sound only", () => {
  const c = changeFor(new SessionStore(), ev("a", "Notification", { message: "Rate limit reached" }));
  assert.deepEqual(reactTo(c, "Notification", ON), { sound: "rate", alert: null, reveal: false });
});

test("a permission card is left to the approval flow", () => {
  const c = changeFor(new SessionStore(), ev("a", "PermissionRequest", BASH));
  assert.deepEqual(reactTo(c, "PermissionRequest", ON), { sound: null, alert: null, reveal: false });
});

test("work shows the compact island; a new session also says hello", () => {
  assert.deepEqual(reactTo(changeFor(new SessionStore(), ev("a", "SessionStart")), "SessionStart", ON), { sound: "work", alert: null, reveal: true });
  for (const name of ["UserPromptSubmit", "PreToolUse"]) {
    const extra = name === "PreToolUse" ? BASH : { prompt: "x" };
    assert.deepEqual(reactTo(changeFor(new SessionStore(), ev("a", name, extra)), name, ON), { sound: null, alert: null, reveal: true }, name);
  }
  const quiet = changeFor(new SessionStore(), ev("a", "PostToolUse", BASH));
  assert.deepEqual(reactTo(quiet, "PostToolUse", ON), { sound: null, alert: null, reveal: false });
});

test("a session that ended needs nothing", () => {
  const s = new SessionStore();
  s.apply(ev("a", "SessionStart"), T0);
  assert.deepEqual(reactTo(s.apply(ev("a", "SessionEnd"), T0), "SessionEnd", ON), { sound: null, alert: null, reveal: false });
});

// ── Which session a card is about ─────────────────────────────────────────────

test("the question card is about the most urgent waiting session", () => {
  const s = new SessionStore();
  assert.equal(waitingSession(s.list()), null);
  s.apply(ev("work", "PreToolUse", BASH), T0);
  s.apply(ev("old", "PreToolUse", ASK), T0 + 1);
  s.apply(ev("new", "PreToolUse", ASK), T0 + 2);
  assert.equal(waitingSession(s.list()).id, "new");
});

test("the finished card is about the latest unseen finish", () => {
  const s = new SessionStore();
  assert.equal(finishedSession(s.list()), null);
  s.apply(ev("first", "Stop"), T0);
  s.apply(ev("second", "Stop"), T0 + 1);
  assert.equal(finishedSession(s.list()).id, "second");
  s.markSeen("second");
  assert.equal(finishedSession(s.list()).id, "first");
});

test("the card says what the session waits for, and where", () => {
  const s = new SessionStore();
  s.apply(ev("q", "PreToolUse", { ...ASK, coucou_host: CURSOR }), T0);
  assert.equal(waitingLine(s.get("q")), "asks in Cursor");

  s.apply(ev("p", "PermissionRequest", { ...BASH, coucou_host: WT }), T0);
  s.handBack("p");
  assert.equal(waitingLine(s.get("p")), "needs permission in Windows Terminal");

  s.apply(ev("i", "Notification", { notification_type: "elicitation_dialog", message: "Pick one", coucou_host: CURSOR }), T0);
  assert.equal(waitingLine(s.get("i")), "needs you in Cursor");

  s.apply(ev("n", "PreToolUse", ASK), T0);
  assert.equal(waitingLine(s.get("n")), "is asking a question", "no host known: no place named");
});

test("a question card is stale once nobody waits any more", () => {
  // Measured: the waiting session was pruned while its card was up, and the
  // empty card then held off the finished card of another session.
  const s = new SessionStore();
  s.apply(ev("a", "PreToolUse", ASK), T0);
  assert.equal(staleCard("question", s.list()), false);
  s.apply(ev("a", "PostToolUse", ASK), T0 + 1);
  assert.equal(staleCard("question", s.list()), true, "answered in the terminal");
  s.apply(ev("b", "PreToolUse", ASK), T0 + 2);
  s.remove("b");
  assert.equal(staleCard("question", s.list()), true, "gone");
  for (const view of ["overview", "approval", "finished", "prompt"]) {
    assert.equal(staleCard(view, s.list()), false, view);
  }
});

// ── Jumping to a session ──────────────────────────────────────────────────────

test("the jump button names where it goes", () => {
  const s = new SessionStore();
  s.apply(ev("c", "PreToolUse", { ...ASK, coucou_host: CURSOR }), T0);
  s.apply(ev("w", "PreToolUse", { ...ASK, coucou_host: WT }), T0);
  s.apply(ev("u", "PreToolUse", ASK), T0);
  assert.equal(jumpLabel(s.get("c")), "Go to Cursor");
  assert.equal(jumpLabel(s.get("w")), "Go to Windows Terminal");
  assert.equal(jumpLabel(s.get("u")), "Go to session");
});

test("a jump that worked says nothing; one that did not says why", () => {
  assert.equal(jumpNote("focused"), null);
  assert.equal(jumpNote("focusedUnsure"), null);
  assert.equal(jumpNote(null), null, "outside the app there is nothing to say");
  for (const outcome of ["flashed", "noWindow", "unknownSession"]) {
    const note = jumpNote(outcome);
    assert.equal(typeof note, "string", outcome);
    assert.ok(note.length > 10, outcome);
  }
  assert.match(jumpNote("flashed"), /taskbar/);
});
