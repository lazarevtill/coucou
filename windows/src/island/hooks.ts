// Claude Code hook events → island state.
// Port of HookServer.processEvent / processPermissionRequest from the macOS app.
// Difference from macOS: no terminal filter. On Windows the hook fires from any
// terminal (Windows Terminal, VS Code, PowerShell…) and all of them are handled.
//
// Claude Code sessions are kept one record each (core/sessions.ts) and the one
// Claude pill shows their sum (core/sessionView.ts). Other agents, tagged with
// `coucou_agent`, each get a pill of their own.

import { Bridge, onEvent } from "../core/bridge";
import { Sound } from "../core/sound";
import { CLAUDE_ID, State } from "../core/state";
import { CELEBRATE_MS, approvalTarget, stepLabel, type HookEvent } from "../core/sessions";
import { reactTo, staleCard } from "../core/sessionView";
import type { Island } from "./island";

/** Clears the approval card if no decision was made before the hook gave up. */
let pendingTimeout: number | null = null;

interface HookPayload extends HookEvent {
  request_id?: string;
  /** Optional agent tag: lowercase, digits and hyphens, ≤ 24 chars. */
  coucou_agent?: string;
}

/** Same rule as HookServer.validateAgent on macOS. "claude" is reserved. */
function validateAgent(raw: string | undefined): string | null {
  if (!raw || raw.length > 24 || raw === "claude") return null;
  if (!/^[a-z0-9-]+$/.test(raw)) return null;
  return raw;
}

const FALLBACK_COLORS = ["#22C55E", "#EAB308", "#60A5FA", "#E879F9"];

function agentColor(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) {
    h = (Math.imul(31, h) + name.charCodeAt(i)) | 0;
  }
  return FALLBACK_COLORS[Math.abs(h) % FALLBACK_COLORS.length];
}

export function registerHookHandlers(island: Island) {
  void onEvent<HookPayload>("hook", (payload) => handleHook(island, payload));
  // A session whose Claude Code process is gone (closed terminal, crash) never
  // sends SessionEnd; the app notices and says so.
  void onEvent<string>("session-ended", (id) => {
    if (!State.sessions.remove(id)) return;
    State.syncClaude();
    if (State.mode === "expanded" && staleCard(State.view, State.sessions.list())) {
      island.setView(State.defaultView());
    }
    State.notify();
  });
}

function handleHook(island: Island, payload: HookPayload) {
  if (State.paused) {
    // Silence here used to cost Claude Code nearly two minutes: the relay waited
    // for a decision from an island that had already decided not to look. Say so,
    // and the terminal takes the question immediately.
    if (payload.request_id) void Bridge.approvalDecline(payload.request_id);
    return;
  }

  // Valid coucou_agent → dynamic "agent_<name>" pill. "claude" is reserved;
  // absent or invalid → Claude Code.
  const agent = validateAgent(payload.coucou_agent);
  if (agent) handleAgent(island, payload, agent);
  else handleClaude(island, payload);
  State.notify();
}

// ── Claude Code ───────────────────────────────────────────────────────────────

function handleClaude(island: Island, payload: HookPayload) {
  const name = payload.hook_event_name ?? "";
  const change = State.sessions.apply(payload, Date.now());

  if (change.declineApproval) {
    // A question, not a permission: Claude Code shows it in the terminal at once.
    if (payload.request_id) void Bridge.approvalDecline(payload.request_id);
  } else if (name === "PermissionRequest" && change.session) {
    openApprovalCard(island, change.session.id, payload);
  }

  State.syncClaude();

  // What is on screen, as far as holding it goes: a question card whose session
  // no longer waits holds nothing.
  const showing = State.mode === "expanded" ? State.view : "overview";
  const stale = staleCard(showing, State.sessions.list());
  const view = State.pendingApproval ? "approval" : stale ? "overview" : showing;
  const reaction = reactTo(change, name, { focused: State.focusId === CLAUDE_ID, view });
  if (reaction.sound) Sound.play(reaction.sound);
  if (reaction.alert) {
    if (State.mode === "expanded") island.setView(reaction.alert);
    else island.alert(reaction.alert);
  } else if (stale) {
    island.setView(State.defaultView());
  } else if (reaction.reveal && State.mode === "hidden") {
    island.reveal();
  }

  // The character celebrates a finish for a while; look again once that is over.
  if (change.became === "finished") {
    window.setTimeout(() => {
      State.syncClaude();
      State.notify();
    }, CELEBRATE_MS + 100);
  }
}

function openApprovalCard(island: Island, sessionId: string, payload: HookPayload) {
  const requestId = payload.request_id ?? "";
  // One card, one request. A second one must never quietly replace the first
  // — that would leave a human staring at request B while request A waits for
  // a decision nobody can give. Hand it straight back to the terminal.
  if (State.pendingApproval && State.pendingApproval.requestId !== requestId) {
    if (requestId) void Bridge.approvalDecline(requestId);
    State.sessions.handBack(sessionId);
    Sound.play("question");
    return;
  }
  if (pendingTimeout != null) window.clearTimeout(pendingTimeout);
  const tool = payload.tool_name ?? "Tool";
  State.pendingApproval = {
    requestId,
    sessionId,
    tool,
    command: approvalTarget(tool, payload.tool_input ?? {}),
  };
  // The relay's short ack window closes in 800 ms; everything below this
  // line is synchronous, so the card really is up by the time it lands.
  if (requestId) void Bridge.approvalAck(requestId);
  State.isPinned = true;
  Sound.play("approval");
  if (State.focusId === CLAUDE_ID) {
    island.alert("approval");
  } else {
    // Another agent holds the view, so the card would yank it away. The badge
    // is the signal instead — but it has to be on screen for that to mean
    // anything, hence the reveal. We just told the relay a human can act.
    island.reveal();
  }
  // Coucou answers within 108 s or not at all; after that the terminal has
  // taken over and the card would be lying.
  pendingTimeout = window.setTimeout(() => {
    pendingTimeout = null;
    const pending = State.pendingApproval;
    if (!pending) return;
    State.pendingApproval = null;
    State.isPinned = false;
    island.dropPin();
    State.sessions.handBack(pending.sessionId);
    State.syncClaude();
    if (State.view === "approval") island.setView(State.defaultView());
    State.notify();
  }, 110_000);
}

// ── Other agents (coucou_agent) ───────────────────────────────────────────────

function handleAgent(island: Island, payload: HookPayload, agent: string) {
  const id = `agent_${agent}`;
  const focused = State.focusId === id;

  /** Alerts force the island open; work events only reveal the compact island. */
  const surface = (view: Parameters<Island["alert"]>[0], isAlert: boolean) => {
    if (State.mode === "expanded") {
      if (isAlert) island.setView(view);
    } else if (isAlert) {
      island.alert(view);
    } else if (State.mode === "hidden") {
      island.reveal();
    }
  };
  const ensurePill = () => State.upsertExternalAgent(id, agent, agentColor(agent));

  switch (payload.hook_event_name ?? "") {
    case "SessionStart":
      ensurePill();
      surface("overview", false);
      Sound.play("work");
      break;

    case "UserPromptSubmit": {
      ensurePill();
      State.updateTask(id, "thinking");
      const asked = payload.prompt ?? payload.message;
      if (asked) State.appendStep(id, asked.slice(0, 60));
      surface("overview", false);
      break;
    }

    case "PreToolUse":
      ensurePill();
      State.updateTask(id, "working");
      State.appendStep(id, stepLabel(payload.tool_name ?? "Tool", payload.tool_input ?? {}));
      surface("overview", false);
      break;

    case "PostToolUse":
      State.updateTask(id, "working");
      break;

    case "PostToolUseFailure":
      State.updateTask(id, "working");
      State.appendStep(id, "⚠ failed");
      break;

    case "Notification": {
      const message = payload.message ?? "";
      const lower = message.toLowerCase();
      if (lower.includes("rate limit") || lower.includes("limite d")) {
        State.updateTask(id, "ratelimit");
        Sound.play("rate");
      } else if (message.endsWith("?")) {
        State.updateTask(id, "question");
        State.appendStep(id, message);
      }
      break;
    }

    case "Stop":
      State.updateTask(id, "finished");
      if (payload.message) State.appendStep(id, payload.message.slice(0, 60));
      Sound.play("finish");
      if (focused) surface("finished", true);
      else State.setPillBadge(id, "finished");
      window.setTimeout(() => State.removeTask(id), 5200);
      break;

    case "StopFailure":
      State.updateTask(id, "error");
      Sound.play("error");
      if (focused) surface("error", true);
      else State.setPillBadge(id, "error");
      break;

    case "SessionEnd":
      State.removeTask(id);
      break;

    case "SubagentStart":
      State.appendStep(id, "+ subagent");
      break;

    case "SubagentStop":
      State.appendStep(id, "• subagent done");
      break;

    case "PermissionRequest":
      // External agents do not get an approval card — showing one would look like
      // a Claude Code request. Decline immediately so the agent re-asks in its
      // terminal. Approval support for other agents will come with Codex support.
      if (payload.request_id) void Bridge.approvalDecline(payload.request_id);
      break;

    default:
      break;
  }
}
