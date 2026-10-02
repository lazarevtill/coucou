// Claude Code sessions, one record each.
//
// Until now every session collapsed onto a single pill: whichever hook event came
// last overwrote the state, so one session finishing marked its neighbour done and
// one SessionEnd cleared them all. This keeps a record per session_id and works out
// what to show from all of them.
//
// Pure on purpose — no DOM, no Tauri, no clock of its own — so it is tested
// directly (windows/tests). The island turns what it says into pills and sounds.
//
// Kept to erasable TypeScript (no enums, no parameter properties) so Node can run
// the tests without a build step.

export type SessionState =
  | "idle"
  | "thinking"
  | "working"
  | "waiting" // asking the human something
  | "approval" // a permission card is up
  | "finished"
  | "error"
  | "ratelimit";

export interface HostInfo {
  kind: "cursor" | "vscode" | "windows-terminal" | "unknown";
  shell: "pwsh" | "powershell" | "cmd" | "bash" | null;
  label: string;
  shellLabel: string | null;
}

/** The part of a hook payload this reads. */
export interface HookEvent {
  hook_event_name?: string;
  session_id?: string;
  cwd?: string;
  message?: string;
  prompt?: string;
  tool_name?: string;
  tool_input?: Record<string, unknown>;
  notification_type?: string;
  last_assistant_message?: string;
  error_message?: string;
  error_type?: string;
  permission_mode?: string;
  /** Added by the app: where the session lives. */
  coucou_host?: HostInfo;
}

export interface SessionInfo {
  id: string;
  cwd: string;
  project: string;
  host: HostInfo | null;
  state: SessionState;
  /** What it is waiting on: the question, or the command being approved. */
  waitingFor: string | null;
  waitingKind: "question" | "permission" | "input" | null;
  steps: string[];
  permissionMode: string | null;
  lastEventAt: number;
  finishedAt: number | null;
  /** Finished or failed, and the human has not looked yet. */
  unseen: boolean;
}

export type Notable = "waiting" | "approval" | "finished" | "error" | "ratelimit";

/** What one event did, for the island to react to. */
export interface Change {
  session: SessionInfo | null;
  removed: boolean;
  /** A transition worth a sound or an alert; `null` for ordinary work. */
  became: Notable | null;
  /** A question arrived as a permission request: answer "no decision" so the terminal shows it. */
  declineApproval: boolean;
}

export interface Summary {
  count: number;
  /** The state the character should show. */
  state: SessionState;
  waiting: number;
  approval: number;
  working: number;
  unseenFinished: number;
  unseenErrors: number;
  /** The session most worth showing, by urgency then recency. */
  top: SessionInfo | null;
  /** What the Claude pill's badge should be, if any. */
  badge: "needs-you" | "error" | "finished" | null;
}

// ── Helpers shared with the island ────────────────────────────────────────────

const PROJECT_ALIASES: Record<string, string> = {
  "notch-buddy": "Notch Buddy",
  notchbuddy: "Notch Buddy",
  notch_buddy: "Notch Buddy",
};

export function aliasProjectName(name: string): string {
  return PROJECT_ALIASES[name.toLowerCase()] ?? name;
}

export function lastPathComponent(p: string): string {
  const cleaned = p.replace(/[\\/]+$/, "");
  const idx = Math.max(cleaned.lastIndexOf("\\"), cleaned.lastIndexOf("/"));
  return idx >= 0 ? cleaned.slice(idx + 1) : cleaned;
}

/** frenchStep() — same labels as the macOS app. */
const TOOL_LABELS: Record<string, string> = {
  Bash: "Exécute",
  Read: "Lit",
  Write: "Écrit",
  Edit: "Modifie",
  Glob: "Cherche",
  Grep: "Recherche",
  WebSearch: "Recherche web",
  WebFetch: "Récupère",
  TodoWrite: "Tâches",
  Task: "Agent",
  LS: "Liste",
  MultiEdit: "Modifie",
  NotebookEdit: "Notebook",
  PowerShell: "Exécute",
};

export function stepLabel(tool: string, input: Record<string, unknown>): string {
  const label = TOOL_LABELS[tool] ?? tool;
  const str = (k: string) => (typeof input[k] === "string" ? (input[k] as string) : null);
  const cmd = str("command");
  if (cmd) return `${label} · ${cmd.slice(0, 40)}`;
  const path = str("path");
  if (path) return `${label} · ${lastPathComponent(path)}`;
  const file = str("file_path");
  if (file) return `${label} · ${lastPathComponent(file)}`;
  const query = str("query");
  if (query) return `${label} · ${query.slice(0, 40)}`;
  return label;
}

/**
 * What the Allow button actually authorises. Approving "Write" tells you nothing
 * — approving `Write · C:\…\.env` tells you everything, and the difference is
 * the whole point of approving from the island rather than blind.
 *
 * Ordered by how specific the field is, so an unfamiliar tool still shows
 * whatever identifying string it carries instead of falling back to its name.
 */
const APPROVAL_FIELDS = [
  "command", // Bash, PowerShell
  "file_path", // Write, Edit, MultiEdit, NotebookEdit
  "path", // Read, LS
  "url", // WebFetch
  "query", // WebSearch
  "pattern", // Glob, Grep
  "prompt", // Task
] as const;

export function approvalTarget(tool: string, input: Record<string, unknown>): string {
  for (const field of APPROVAL_FIELDS) {
    const value = input[field];
    if (typeof value === "string" && value.trim()) {
      return `${tool} · ${value.trim()}`;
    }
  }
  return tool;
}

// ── The store ─────────────────────────────────────────────────────────────────

/** Steps kept per session — what the ticker can scroll through. */
const STEP_CAP = 20;
/** How long a fresh finish keeps the character celebrating. */
export const CELEBRATE_MS = 5200;
const SNIPPET = 60;

/** Notification types that mean a human is needed (from the hooks reference). */
const NEEDS_HUMAN = new Set([
  "permission_prompt",
  "elicitation_dialog",
  "elicitation_url_dialog",
  "agent_needs_input",
]);

const ASK_TOOL = "AskUserQuestion";

/** A session's identity. Older relays sent no session_id; those group by folder. */
function keyOf(ev: HookEvent): string {
  return ev.session_id ? ev.session_id : `cwd:${ev.cwd ?? ""}`;
}

/** The first question of an AskUserQuestion call. */
function questionText(input: Record<string, unknown> | undefined): string {
  const questions = input?.questions;
  if (Array.isArray(questions)) {
    const first = questions[0] as { question?: unknown } | undefined;
    if (first && typeof first.question === "string" && first.question.trim()) {
      return first.question.trim().slice(0, 160);
    }
  }
  return "Claude is asking a question";
}

/** Lower is more urgent. */
function rank(s: SessionInfo): number {
  switch (s.state) {
    case "approval": return 0;
    case "waiting": return 1;
    case "error": return s.unseen ? 2 : 5;
    case "ratelimit": return 3;
    case "working":
    case "thinking": return 4;
    case "finished": return s.unseen ? 5 : 6;
    default: return 7;
  }
}

export class SessionStore {
  private map = new Map<string, SessionInfo>();

  get size(): number {
    return this.map.size;
  }

  get(id: string): SessionInfo | undefined {
    return this.map.get(id);
  }

  remove(id: string): boolean {
    return this.map.delete(id);
  }

  /** Marks one session's finish/failure as seen, or all of them. */
  markSeen(id?: string): void {
    for (const s of id === undefined ? this.map.values() : [this.map.get(id)]) {
      if (s) s.unseen = false;
    }
  }

  /** The human answered the permission card: the session is working again. */
  resolveApproval(id: string): void {
    const s = this.map.get(id);
    if (s && s.state === "approval") {
      s.state = "working";
      clearWaiting(s);
    }
  }

  /**
   * The card could not take this request (another one was up, or nobody answered
   * in time), so Claude Code is asking in its terminal now: still waiting on the
   * human, just somewhere else.
   */
  handBack(id: string): void {
    const s = this.map.get(id);
    if (s && s.state === "approval") s.state = "waiting";
  }

  /** Most urgent first, then most recently active. */
  list(): SessionInfo[] {
    return [...this.map.values()].sort((a, b) => rank(a) - rank(b) || b.lastEventAt - a.lastEventAt);
  }

  summary(now: number): Summary {
    const all = this.list();
    const top = all[0] ?? null;
    let state: SessionState = top?.state ?? "idle";
    if (state === "finished" && (top?.finishedAt == null || now - top.finishedAt > CELEBRATE_MS)) {
      state = "idle";
    }
    const count = (f: (s: SessionInfo) => boolean) => all.filter(f).length;
    const waiting = count((s) => s.state === "waiting");
    const approval = count((s) => s.state === "approval");
    const unseenFinished = count((s) => s.state === "finished" && s.unseen);
    const unseenErrors = count((s) => s.state === "error" && s.unseen);
    return {
      count: all.length,
      state,
      waiting,
      approval,
      working: count((s) => s.state === "working" || s.state === "thinking"),
      unseenFinished,
      unseenErrors,
      top,
      badge:
        waiting + approval > 0 ? "needs-you"
        : unseenErrors > 0 ? "error"
        : unseenFinished > 0 ? "finished"
        : null,
    };
  }

  apply(ev: HookEvent, now: number): Change {
    const name = ev.hook_event_name ?? "";

    if (name === "SessionEnd") {
      return { session: null, removed: this.map.delete(keyOf(ev)), became: null, declineApproval: false };
    }

    const s = this.touch(ev, now);
    const before = s.state;
    const tool = ev.tool_name ?? "Tool";
    const input = ev.tool_input ?? {};
    let became: Notable | null = null;
    let declineApproval = false;

    switch (name) {
      case "SessionStart":
        // A compaction restarts the session mid-turn; do not wipe real activity.
        if (s.state !== "working" && s.state !== "thinking") {
          s.state = "idle";
          clearWaiting(s);
          s.unseen = false;
          s.finishedAt = null;
        }
        break;

      case "UserPromptSubmit": {
        s.state = "thinking";
        clearWaiting(s);
        s.unseen = false;
        s.finishedAt = null;
        const asked = ev.prompt ?? ev.message;
        if (asked) pushStep(s, asked.slice(0, SNIPPET));
        break;
      }

      case "PreToolUse":
        if (tool === ASK_TOOL) {
          setWaiting(s, "question", questionText(input));
          if (before !== "waiting") became = "waiting";
        } else {
          // An open permission card is not closed by a neighbouring tool starting.
          if (s.state !== "approval") {
            s.state = "working";
            clearWaiting(s);
          }
          s.unseen = false;
          s.finishedAt = null;
          pushStep(s, stepLabel(tool, input));
        }
        break;

      case "PostToolUse":
      case "PostToolUseFailure":
        // After a Stop or a failure this is a race, not a new turn.
        if (s.state !== "finished" && s.state !== "error") {
          s.state = "working";
          clearWaiting(s);
          if (name === "PostToolUseFailure") pushStep(s, "⚠ failed");
        }
        break;

      case "Notification": {
        const message = ev.message ?? "";
        const lower = message.toLowerCase();
        const type = ev.notification_type;
        if (lower.includes("rate limit") || lower.includes("limite d")) {
          s.state = "ratelimit";
          if (before !== "ratelimit") became = "ratelimit";
        } else if (type !== undefined ? NEEDS_HUMAN.has(type) : message.endsWith("?")) {
          // The card for a permission request already says everything, and a
          // session already waiting has said more precisely what for (Claude
          // Code follows a question with a generic "needs your permission").
          if (s.state !== "approval" && s.state !== "waiting") {
            setWaiting(s, type === "permission_prompt" ? "permission" : "input", message);
            if (before !== "waiting") became = "waiting";
          }
        }
        break;
      }

      case "PermissionRequest":
        if (tool === ASK_TOOL) {
          // A question is not a permission. Hand it back to the terminal.
          setWaiting(s, "question", questionText(input));
          declineApproval = true;
          if (before !== "waiting") became = "waiting";
        } else {
          s.state = "approval";
          s.waitingKind = "permission";
          s.waitingFor = approvalTarget(tool, input);
          if (before !== "approval") became = "approval";
        }
        break;

      case "Stop": {
        s.state = "finished";
        s.finishedAt = now;
        s.unseen = true;
        clearWaiting(s);
        const said = ev.last_assistant_message ?? ev.message;
        if (said) pushStep(s, said.slice(0, SNIPPET));
        became = "finished";
        break;
      }

      case "StopFailure":
        s.state = "error";
        s.unseen = true;
        clearWaiting(s);
        pushStep(s, (ev.error_message ?? ev.error_type ?? "error").slice(0, SNIPPET));
        became = "error";
        break;

      case "SubagentStart":
        pushStep(s, "+ subagent");
        break;

      case "SubagentStop":
        pushStep(s, "• subagent done");
        break;

      default:
        break;
    }

    return { session: s, removed: false, became, declineApproval };
  }

  /** The record for an event's session, created if it is new, with what the event says about it. */
  private touch(ev: HookEvent, now: number): SessionInfo {
    const id = keyOf(ev);
    let s = this.map.get(id);
    if (!s) {
      s = {
        id, cwd: "", project: "Session", host: null, state: "idle",
        waitingFor: null, waitingKind: null, steps: [], permissionMode: null,
        lastEventAt: now, finishedAt: null, unseen: false,
      };
      this.map.set(id, s);
    }
    if (ev.cwd) {
      s.cwd = ev.cwd;
      s.project = aliasProjectName(lastPathComponent(ev.cwd) || "Session");
    }
    if (ev.coucou_host) s.host = ev.coucou_host;
    if (ev.permission_mode) s.permissionMode = ev.permission_mode;
    s.lastEventAt = now;
    return s;
  }
}

function clearWaiting(s: SessionInfo) {
  s.waitingFor = null;
  s.waitingKind = null;
}

function setWaiting(s: SessionInfo, kind: NonNullable<SessionInfo["waitingKind"]>, text: string) {
  s.state = "waiting";
  s.waitingKind = kind;
  s.waitingFor = text;
}

function pushStep(s: SessionInfo, step: string) {
  s.steps.push(step);
  if (s.steps.length > STEP_CAP) s.steps.shift();
}
