// What the island shows and does for the Claude Code sessions.
//
// sessions.ts keeps one record per session; this turns those records into the
// one Claude pill (name, face, steps, badge), decides what an event should sound
// like and whether it opens the island, and words what a jump to a session's
// window reports back. Pure, like sessions.ts, so it is tested directly.

import type { Change, SessionInfo, SessionState, Summary } from "./sessions";
import type { BotStateName, IslandViewName } from "./layout";
import type { PillBadge } from "./state";
import type { SoundName } from "./sound";

/** The character has no "waiting" face; a question is what it is waiting on. */
export function botState(s: SessionState): BotStateName {
  return s === "waiting" ? "question" : s;
}

export interface ClaudePill {
  /** The leading session's project, shown next to the character. */
  name: string;
  state: BotStateName;
  steps: string[];
  cwd: string | null;
  badge: PillBadge | null;
  /** The pill's own label: "Claude Code", or "Claude Code · 3" for three sessions. */
  pillLabel: string;
  /** The grey line beside the project: where it runs, how many, bypass. */
  toolLabel: string;
}

const BADGES: Record<NonNullable<Summary["badge"]>, PillBadge> = {
  "needs-you": "approval",
  error: "error",
  finished: "finished",
};

export function claudePill(sum: Summary): ClaudePill {
  const top = sum.top;
  const tool = ["Claude Code"];
  if (top?.host && top.host.kind !== "unknown") tool.push(top.host.label);
  if (sum.count > 1) tool.push(`${sum.count} sessions`);
  if (top?.permissionMode === "bypassPermissions") tool.push("bypass");
  return {
    name: top?.project ?? "Claude Code",
    state: botState(sum.state),
    steps: top ? [...top.steps] : [],
    cwd: top?.cwd || null,
    badge: sum.badge ? BADGES[sum.badge] : null,
    pillLabel: sum.count > 1 ? `Claude Code · ${sum.count}` : "Claude Code",
    toolLabel: tool.join(" · "),
  };
}

export interface Reaction {
  sound: SoundName | null;
  /** Open the island on this card. */
  alert: "question" | "finished" | "error" | null;
  /** Show the compact island if it is hidden, without taking anything over. */
  reveal: boolean;
}

/** Cards that wait on the human: nothing else may replace them. */
const HOLDING: ReadonlySet<IslandViewName> = new Set(["approval", "question"]);

const NOTHING: Reaction = { sound: null, alert: null, reveal: false };

/**
 * What the island does about one event. `focused` is whether the Claude pill is
 * the one in front; when it is not, the badge carries the news instead of a card.
 * Permission cards are not decided here: the approval flow owns them.
 */
export function reactTo(change: Change, eventName: string, ctx: { focused: boolean; view: IslandViewName }): Reaction {
  if (!change.session) return NOTHING;
  const holding = HOLDING.has(ctx.view);
  switch (change.became) {
    case "waiting": {
      // A newer question may replace an older one, never a permission card.
      const canShow = ctx.focused && ctx.view !== "approval";
      return { sound: "question", alert: canShow ? "question" : null, reveal: !canShow };
    }
    case "finished":
    case "error": {
      const canShow = ctx.focused && !holding;
      return {
        sound: change.became === "finished" ? "finish" : "error",
        alert: canShow ? (change.became === "finished" ? "finished" : "error") : null,
        reveal: false,
      };
    }
    case "ratelimit":
      return { sound: "rate", alert: null, reveal: false };
    case "approval":
      return NOTHING;
    default:
      break;
  }
  switch (eventName) {
    case "SessionStart":
      return { sound: "work", alert: null, reveal: true };
    case "UserPromptSubmit":
    case "PreToolUse":
      return { sound: null, alert: null, reveal: true };
    default:
      return NOTHING;
  }
}

/** The session the question card is about. */
export function waitingSession(list: SessionInfo[]): SessionInfo | null {
  return list.find((s) => s.state === "waiting") ?? null;
}

/** The session the finished card is about: the latest finish nobody has looked at. */
export function finishedSession(list: SessionInfo[]): SessionInfo | null {
  return list.find((s) => s.state === "finished" && s.unseen) ?? null;
}

/** The card on screen is about a question nobody is waiting on any more. */
export function staleCard(view: IslandViewName, list: SessionInfo[]): boolean {
  return view === "question" && waitingSession(list) === null;
}

/** The grey line on the question card: what the session waits for, and where. */
export function waitingLine(s: SessionInfo): string {
  const where = s.host && s.host.kind !== "unknown" ? s.host.label : null;
  if (!where) return "is asking a question";
  switch (s.waitingKind) {
    case "permission": return `needs permission in ${where}`;
    case "input": return `needs you in ${where}`;
    default: return `asks in ${where}`;
  }
}

/** The button that brings a session's window to the front. */
export function jumpLabel(s: SessionInfo): string {
  return s.host && s.host.kind !== "unknown" ? `Go to ${s.host.label}` : "Go to session";
}

/** What to tell the human after a jump, or null when the window is in front. */
export function jumpNote(outcome: string | null): string | null {
  switch (outcome) {
    case "flashed":
      return "Windows wouldn't switch — the taskbar button is flashing.";
    case "noWindow":
      return "That session has no window to switch to.";
    case "unknownSession":
      return "Coucou lost track of that session.";
    default:
      return null;
  }
}
