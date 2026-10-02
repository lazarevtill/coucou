// The Windows shell's view of the sessions: what the tray icon and its tooltip
// say, the rows of the tray flyout, and which events become toast notifications.
// Pure, like sessions.ts, so it is tested directly.

import type { Notable, SessionInfo, SessionState, Summary } from "./sessions";
import { botState, jumpLabel, waitingLine } from "./sessionView.ts";
import type { BotStateName } from "./layout";

export type TrayState = "idle" | "working" | "attention" | "error" | "done" | "paused";

/** The badge on the tray icon: the most important thing going on. */
export function trayState(sum: Summary, paused: boolean): TrayState {
  if (paused) return "paused";
  if (sum.waiting + sum.approval > 0) return "attention";
  if (sum.unseenErrors > 0) return "error";
  if (sum.working > 0) return "working";
  if (sum.unseenFinished > 0) return "done";
  return "idle";
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

function where(s: SessionInfo): string {
  return s.host && s.host.kind !== "unknown" ? ` in ${s.host.label}` : "";
}

/** The tray icon's tooltip. Rust cuts it to what Windows shows (127 units). */
export function tooltipText(sum: Summary, paused: boolean): string {
  if (paused) return "Coucou — paused";
  if (sum.count === 0) return "Coucou — no Claude Code sessions";
  const parts = [plural(sum.count, "session")];
  const top = sum.top;
  if (top && (top.state === "waiting" || top.state === "approval")) {
    parts.push(`${top.project} needs you${where(top)}`);
  }
  if (sum.working > 0) parts.push(`${sum.working} working`);
  if (sum.unseenErrors > 0) parts.push(`${sum.unseenErrors} failed`);
  if (sum.unseenFinished > 0) parts.push(`${sum.unseenFinished} finished`);
  return `Coucou — ${parts.join(" · ")}`;
}

/** The line under the flyout's title. */
export function headline(sum: Summary): string {
  if (sum.count === 0) return "No Claude Code sessions";
  const needs = sum.waiting + sum.approval;
  if (needs > 0) return `${plural(sum.count, "session")} · ${needs} need${needs === 1 ? "s" : ""} you`;
  if (sum.working > 0) return `${plural(sum.count, "session")} · working`;
  return plural(sum.count, "session");
}

export interface SessionRow {
  id: string;
  project: string;
  /** "Windows Terminal", "Cursor"… or null when unknown. */
  host: string | null;
  shell: string | null;
  state: SessionState;
  label: string;
  /** What it waits for, or the last thing it did. */
  detail: string;
  bypass: boolean;
  unseen: boolean;
  /** Needs the human now. */
  attention: boolean;
  /** Nothing new to see. */
  dim: boolean;
  jumpLabel: string;
}

function stateLabel(s: SessionInfo): string {
  switch (s.state) {
    case "approval": return "Needs permission";
    case "waiting":
      return s.waitingKind === "question" ? "Asks you" : s.waitingKind === "permission" ? "Needs permission" : "Needs you";
    case "thinking": return "Thinking";
    case "working": return "Working";
    case "finished": return "Finished";
    case "error": return "Failed";
    case "ratelimit": return "Rate limited";
    default: return "Idle";
  }
}

export function sessionRows(list: SessionInfo[]): SessionRow[] {
  return list.map((s) => {
    const known = s.host && s.host.kind !== "unknown" ? s.host : null;
    return {
      id: s.id,
      project: s.project,
      host: known?.label ?? null,
      shell: known?.shellLabel ?? null,
      state: s.state,
      label: stateLabel(s),
      detail: s.waitingFor ?? s.steps.at(-1) ?? "",
      bypass: s.permissionMode === "bypassPermissions",
      unseen: s.unseen,
      attention: s.state === "waiting" || s.state === "approval",
      dim: s.state === "idle" || ((s.state === "finished" || s.state === "error") && !s.unseen),
      jumpLabel: jumpLabel(s),
    };
  });
}

export interface ServiceRow {
  id: string;
  name: string;
  color: string;
  status: "ok" | "error" | "off";
  detail: string;
  /** Opened when the row is clicked (plugins with a link). */
  link?: string;
}

/** What a plugin from the folder last reported (Rust's PluginStatus). */
export interface PluginStatus {
  id: string;
  name: string;
  color: string;
  status: "ok" | "error" | "waiting" | "off" | "changed" | "invalid";
  headline: string | null;
  items: { title: string; detail: string }[];
  error: string | null;
  link: string | null;
}

/** Rows for the plugins that are on, or that need a look (changed since approved). */
export function pluginRows(statuses: Record<string, PluginStatus>): ServiceRow[] {
  const rows: ServiceRow[] = [];
  for (const p of Object.values(statuses)) {
    const base = { id: p.id, name: p.name, color: p.color, ...(p.link ? { link: p.link } : {}) };
    switch (p.status) {
      case "ok":
        rows.push({ ...base, status: "ok", detail: p.headline ?? "Connected" });
        break;
      case "error":
      case "changed":
        rows.push({ ...base, status: "error", detail: p.error ?? "Not working" });
        break;
      case "waiting":
        rows.push({ ...base, status: "off", detail: "Starting…" });
        break;
      default:
        break; // off, or not a plugin yet: that is for Settings → Plugins
    }
  }
  return rows;
}

interface TaskLike {
  id: string;
  name: string;
  color: string;
  isIntegration: boolean;
}

interface IntegrationLike {
  configured: boolean;
  error: string | null;
}

/** The integrations next to the sessions — Claude Code itself is the session list. */
export function serviceRows(tasks: TaskLike[], info: Record<string, IntegrationLike | undefined>): ServiceRow[] {
  return tasks
    .filter((t) => t.isIntegration && t.id !== "integration_claude")
    .map((t) => {
      const i = info[t.id];
      if (!i?.configured) return { id: t.id, name: t.name, color: t.color, status: "off", detail: "Not set up" };
      if (i.error) return { id: t.id, name: t.name, color: t.color, status: "error", detail: i.error };
      return { id: t.id, name: t.name, color: t.color, status: "ok", detail: "Connected" };
    });
}

export interface ToastSpec {
  /** One toast per session: a new one replaces the old. */
  tag: string;
  title: string;
  body: string;
  /** The session the button goes to. */
  jump: string | null;
  jumpLabel: string;
}

export interface ToastContext {
  /** Settings → Notifications. */
  notify: "needsYou" | "all" | "off";
  paused: boolean;
  /** The island already opened a card for this. */
  islandAlerted: boolean;
}

/**
 * The toast for something that just happened to a session, or null. Permission
 * cards never become toasts: what is approved has to be seen whole, in a window.
 */
export function toastFor(kind: Notable, s: SessionInfo, ctx: ToastContext): ToastSpec | null {
  if (ctx.notify === "off" || ctx.paused || ctx.islandAlerted) return null;
  const jump = { jump: s.id, jumpLabel: jumpLabel(s) };
  switch (kind) {
    case "waiting":
      return { tag: s.id, title: `${s.project} ${waitingLine(s)}`, body: s.waitingFor ?? "", ...jump };
    case "error":
      return { tag: s.id, title: `${s.project} stopped on an error`, body: s.steps.at(-1) ?? "", ...jump };
    case "finished":
      if (ctx.notify !== "all") return null;
      return { tag: s.id, title: `${s.project} finished`, body: s.steps.at(-1) ?? "", ...jump };
    default:
      return null;
  }
}

/** Toasts to take back: their session no longer needs anyone, or is gone. */
export function staleToasts(shown: Map<string, Notable>, list: SessionInfo[]): string[] {
  const byId = new Map(list.map((s) => [s.id, s]));
  const stale: string[] = [];
  for (const [tag, kind] of shown) {
    const s = byId.get(tag);
    const still =
      s !== undefined &&
      (kind === "waiting" ? s.state === "waiting" || s.state === "approval" : s.state === kind && s.unseen);
    if (!still) stale.push(tag);
  }
  return stale;
}

// ── The snapshot the island publishes ─────────────────────────────────────────

export interface ApprovalCard {
  requestId: string;
  sessionId: string;
  project: string;
  host: string | null;
  /** Exactly what Allow authorises: the command, the file, the URL. */
  target: string;
}

/** Everything the tray icon and the flyout show, in one object. */
export interface ShellSnapshot {
  trayState: TrayState;
  tooltip: string;
  headline: string;
  paused: boolean;
  island: "on" | "off";
  /** Mochi's face in the flyout. */
  botState: BotStateName;
  approval: ApprovalCard | null;
  sessions: SessionRow[];
  services: ServiceRow[];
}

interface StoreLike {
  list(): SessionInfo[];
  summary(now: number): Summary;
  get(id: string): SessionInfo | undefined;
}

export interface SnapshotInput {
  store: StoreLike;
  now: number;
  paused: boolean;
  island: "on" | "off";
  pending: { requestId: string; sessionId: string; command: string; tool: string } | null;
  tasks: TaskLike[];
  integrations: Record<string, IntegrationLike | undefined>;
  /** Plugins from the folder. */
  plugins?: Record<string, PluginStatus>;
}

export function buildSnapshot(input: SnapshotInput): ShellSnapshot {
  const sum = input.store.summary(input.now);
  const pending = input.pending;
  let approval: ApprovalCard | null = null;
  if (pending) {
    const s = pending.sessionId ? input.store.get(pending.sessionId) : undefined;
    const known = s?.host && s.host.kind !== "unknown" ? s.host : null;
    approval = {
      requestId: pending.requestId,
      sessionId: pending.sessionId,
      project: s?.project ?? "Claude Code",
      host: known?.label ?? null,
      target: pending.command || pending.tool,
    };
  }
  return {
    trayState: trayState(sum, input.paused),
    tooltip: tooltipText(sum, input.paused),
    headline: headline(sum),
    paused: input.paused,
    island: input.island,
    botState: botState(sum.state),
    approval,
    sessions: sessionRows(input.store.list()),
    services: [...serviceRows(input.tasks, input.integrations), ...pluginRows(input.plugins ?? {})],
  };
}
