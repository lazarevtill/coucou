// The island's side of the Windows shell. The island page owns the sessions and
// the permission card; this publishes what the tray icon and the flyout show,
// carries out what is clicked in the flyout, and raises and takes back toasts.

import { Bridge, onEvent, type ShellAction } from "../core/bridge";
import type { Notable, SessionInfo } from "../core/sessions";
import { buildSnapshot, staleToasts, toastFor } from "../core/shell";
import { State } from "../core/state";
import type { Island } from "./island";

/** Toasts on screen or in the notification centre, by session, and what for. */
const shownToasts = new Map<string, Notable>();

let lastPublished = "";
let publishTimer: number | null = null;

/** Pause has to reach Rust too, or the pollers keep calling out. */
export function setPaused(island: Island, on: boolean) {
  if (State.paused === on) return;
  State.paused = on;
  void Bridge.setPaused(on);
  if (on) island.fsm.forceHidden();
  else island.reveal();
  State.notify();
}

export function startShellLink(island: Island) {
  State.subscribe(schedulePublish);
  schedulePublish();

  void onEvent<ShellAction>("shell-action", (action) => {
    switch (action.kind) {
      case "decide":
        island.decide(action.decision, action.requestId);
        break;
      case "seen":
        State.sessions.markSeen(action.id);
        State.syncClaude();
        State.notify();
        break;
      case "pause":
        setPaused(island, !State.paused);
        break;
      case "open":
        if (action.focus) State.setFocus(action.focus);
        island.openExplicit(action.view);
        break;
    }
  });

  void onEvent<boolean>("flyout-visible", (visible) => {
    State.flyoutOpen = visible;
    // With the island off, the flyout was the only place the card could be
    // answered. Gone from the screen, nobody can: the terminal asks right away.
    if (!visible && !island.enabled) handBackPending();
  });
}

/** Coalesces bursts of changes into one publish, and skips unchanged ones. */
function schedulePublish() {
  if (publishTimer != null) return;
  publishTimer = window.setTimeout(() => {
    publishTimer = null;
    const snapshot = buildSnapshot({
      store: State.sessions,
      now: Date.now(),
      paused: State.paused,
      island: State.settings.island,
      pending: State.pendingApproval,
      tasks: State.tasks,
      integrations: State.integrations,
    });
    const text = JSON.stringify(snapshot);
    if (text === lastPublished) return;
    lastPublished = text;
    void Bridge.publishShell(snapshot);
  }, 60);
}

/** Raises the toast for something that happened to a session, if one is due. */
export function toast(kind: Notable, session: SessionInfo, islandAlerted: boolean) {
  const spec = toastFor(kind, session, {
    notify: State.settings.notifications,
    paused: State.paused,
    islandAlerted,
  });
  if (!spec) return;
  shownToasts.set(spec.tag, kind);
  void Bridge.showToast(spec);
}

/** Takes back the toasts whose session no longer needs anyone. */
export function clearStaleToasts() {
  for (const tag of staleToasts(shownToasts, State.sessions.list())) {
    shownToasts.delete(tag);
    void Bridge.clearToast(tag);
  }
}

/**
 * The permission card cannot be answered here (nothing on screen shows it):
 * decline, so Claude Code asks in its terminal, and point there with a toast.
 */
export function handBackPending() {
  const pending = State.pendingApproval;
  if (!pending) return;
  State.pendingApproval = null;
  State.isPinned = false;
  if (pending.requestId) void Bridge.approvalDecline(pending.requestId);
  State.sessions.handBack(pending.sessionId);
  State.syncClaude();
  const s = State.sessions.get(pending.sessionId);
  if (s) toast("waiting", s, false);
  State.notify();
}
