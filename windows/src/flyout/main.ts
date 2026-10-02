// The tray flyout. It owns nothing: it draws the snapshot the island publishes
// and sends what is clicked back to it (see island/shellLink.ts). Jumping to a
// session's window is the one thing it does itself, straight through Rust.
//
// Rows are updated in place rather than rebuilt: snapshots arrive while a
// session works, and a button replaced between mouse-down and mouse-up would
// swallow the click.

import "./flyout.css";
import { Bridge, onEvent, type ShellAction } from "../core/bridge";
import type { LineKind } from "../core/edits";
import type { ApprovalCard, ServiceRow, SessionRow, ShellSnapshot } from "../core/shell";
import { jumpNote } from "../core/sessionView";
import type { AgentTask } from "../core/state";
import { createMiniBot, syncMiniBotStates, tickMiniBots } from "../mochi/minibots";
import { ICONS } from "../views/icons";
import { clear, h, svg } from "../views/dom";

const PAUSE = "M7 5h3.5v14H7V5zm6.5 0H17v14h-3.5V5z";
const PLAY = "M8 5.5v13l10.5-6.5L8 5.5z";

const root = document.getElementById("flyout-root")!;

// ── Header ────────────────────────────────────────────────────────────────────

const mochi: AgentTask = {
  id: "flyout-mochi", name: "Coucou", color: "#F5F6F8", state: "idle",
  stepIndex: 0, steps: [], source: "claudeCode", isIntegration: false,
};
const avatar = h("div", { class: "avatar" }, createMiniBot(mochi, 26));
const title = h("div", { class: "title", text: "Claude Code" });
const headline = h("div", { class: "headline", text: "" });
const pauseBtn = h("button", { class: "icon", title: "Pause", onclick: () => act({ kind: "pause" }) });
const settingsBtn = h("button", {
  class: "icon", title: "Settings",
  onclick: () => {
    void Bridge.openSettingsWindow();
    void Bridge.flyoutHide();
  },
}, svg(ICONS.gear, 16));
const header = h("header", {},
  avatar,
  h("div", { class: "heading" }, title, headline),
  h("div", { class: "tools" }, pauseBtn, settingsBtn),
);

// ── Body ──────────────────────────────────────────────────────────────────────

const approvalSlot = h("div", { class: "approval-slot" });
const note = h("div", { class: "note", role: "status" });
const sessionList = h("div", { class: "list", role: "list" });
const empty = h("div", { class: "empty" },
  h("div", { class: "empty-title", text: "No Claude Code sessions" }),
  h("div", { class: "empty-sub", text: "Start claude in a terminal or in Cursor and it shows up here." }),
);
const servicesTitle = h("div", { class: "section", text: "Services" });
const serviceList = h("div", { class: "list services", role: "list" });
const body = h("main", {}, approvalSlot, note, h("div", { class: "section", text: "Sessions" }), sessionList, empty, servicesTitle, serviceList);

const footer = h("footer", {},
  h("button", { class: "action", onclick: () => open("prompt") }, svg(ICONS.bubble, 14), h("span", { text: "Ask Claude" })),
  h("button", { class: "action", onclick: () => open("upload") }, svg(ICONS.plus, 14), h("span", { text: "Drop a file" })),
  h("button", { class: "action", onclick: () => open("overview") }, svg(ICONS.stack, 14), h("span", { text: "Island" })),
);

root.append(header, body, footer);

// ── Actions ───────────────────────────────────────────────────────────────────

function act(action: ShellAction) {
  void Bridge.shellAction(action);
}

function open(view: "prompt" | "upload" | "overview", focus?: string) {
  act({ kind: "open", view, focus });
  void Bridge.flyoutHide();
}

let noteTimer: number | null = null;

function showNote(text: string | null) {
  if (noteTimer != null) window.clearTimeout(noteTimer);
  note.textContent = text ?? "";
  note.classList.toggle("on", !!text);
  if (text) noteTimer = window.setTimeout(() => showNote(null), 4000);
}

async function jump(id: string) {
  const result = await Bridge.focusSession(id);
  const outcome = result?.outcome ?? null;
  act({ kind: "seen", id });
  showNote(jumpNote(outcome));
}

// ── Rendering ─────────────────────────────────────────────────────────────────

let approvalKey = "";

function renderApproval(card: ApprovalCard | null) {
  const key = card ? `${card.requestId}` : "";
  if (key === approvalKey) return;
  approvalKey = key;
  clear(approvalSlot);
  if (!card) return;
  const where = card.host ? ` · ${card.host}` : "";
  approvalSlot.append(
    h("section", { class: "approval", role: "alertdialog", "aria-label": "Permission request" },
      h("div", { class: "approval-who", text: `${card.project} needs permission${where}` }),
      h("code", { class: "approval-target", text: card.target }),
      h("div", { class: "approval-buttons" },
        h("button", { class: "secondary", text: "Deny", onclick: () => act({ kind: "decide", requestId: card.requestId, decision: "deny" }) }),
        h("button", { class: "primary", text: "Allow", onclick: () => act({ kind: "decide", requestId: card.requestId, decision: "allow" }) }),
      ),
    ),
  );
}

interface RowParts {
  /** The row and, under it, its code view. */
  el: HTMLElement;
  row: HTMLElement;
  project: HTMLElement;
  chips: HTMLElement;
  state: HTMLElement;
  detail: HTMLElement;
  jump: HTMLButtonElement;
  editToggle: HTMLButtonElement;
  code: HTMLElement;
  codeKey: string;
}

const rows = new Map<string, RowParts>();
/** Sessions whose code view is open. Kept while the flyout lives. */
const expanded = new Set<string>();

function makeRow(id: string): RowParts {
  const project = h("span", { class: "project" });
  const chips = h("span", { class: "chips" });
  const state = h("div", { class: "state" });
  const detail = h("div", { class: "detail" });
  const jumpBtn = h("button", { class: "jump", onclick: (e: Event) => { e.stopPropagation(); void jump(id); } }, svg(ICONS.arrowUpRight, 14));
  // Opening the code view is its own control: a click on the row still jumps.
  const editToggle = h("button", {
    class: "edit-toggle",
    onclick: (e: Event) => {
      e.stopPropagation();
      if (expanded.has(id)) expanded.delete(id);
      else expanded.add(id);
      if (lastSnapshot) renderSessions(lastSnapshot.sessions);
    },
  });
  editToggle.addEventListener("keydown", (e) => e.stopPropagation());
  const row = h("div", { class: "row", tabindex: "0", onclick: () => void jump(id) },
    h("i", { class: "status" }),
    h("div", { class: "text" }, h("div", { class: "line" }, project, chips), state, detail, editToggle),
    jumpBtn,
  );
  row.addEventListener("keydown", (e) => {
    if (e.key === "Enter") void jump(id);
  });
  const code = h("div", { class: "code" });
  const el = h("div", { class: "session", role: "listitem" }, row, code);
  return { el, row, project, chips, state, detail, jump: jumpBtn, editToggle, code, codeKey: "" };
}

const SIGN: Record<LineKind, string> = { ctx: " ", del: "−", add: "+", gap: "⋯" };

/** The toggle's label: the file and how much changed. */
function renderToggle(parts: RowParts, r: SessionRow) {
  const e = r.edit;
  parts.editToggle.style.display = e ? "" : "none";
  if (!e) return;
  const open = expanded.has(r.id);
  clear(parts.editToggle);
  parts.editToggle.append(
    ...present(
      h("span", { class: "badge", text: e.badge }),
      h("span", { class: "fname", text: e.name }),
      h("span", { class: "count add", text: `+${e.added}` }),
      h("span", { class: "count del", text: `−${e.removed}` }),
      e.status === "failed" ? h("span", { class: "flag", text: "failed" }) : null,
      svg(ICONS.chevronRight, 12),
    ),
  );
  parts.editToggle.classList.toggle("open", open);
  parts.editToggle.setAttribute("aria-expanded", String(open));
  parts.editToggle.title = open ? "Hide the change" : "Show the change";
}

function stepIcon(kind: "done" | "current" | "waiting" | "error") {
  if (kind === "current") return h("i", { class: "spinner" });
  if (kind === "waiting") return svg(ICONS.timer, 12);
  if (kind === "error") return svg(ICONS.bang, 12);
  return svg(ICONS.check, 12);
}

/** The code view: the last steps, then the change, line by line. Text only — never HTML. */
function renderCode(parts: RowParts, r: SessionRow) {
  const e = r.edit;
  const open = !!e && expanded.has(r.id);
  parts.el.classList.toggle("open", open);
  const key = open ? JSON.stringify([e, r.editPath, r.steps, r.state, r.attention]) : "";
  if (key === parts.codeKey) return;
  parts.codeKey = key;
  clear(parts.code);
  if (!open || !e) return;

  const last = r.steps.length - 1;
  const lastKind = r.state === "working" || r.state === "thinking" ? "current" : r.attention ? "waiting" : r.state === "error" ? "error" : "done";
  const steps = h("ol", { class: "steps" },
    ...r.steps.map((s, i) => h("li", { class: i === last ? lastKind : "done" }, stepIcon(i === last ? lastKind : "done"), h("span", { text: s }))),
  );

  const tab = h("div", { class: "tab" },
    h("span", { class: "badge", text: e.badge }),
    h("span", { class: "fname", text: e.name }),
    e.status === "pending" ? h("i", { class: "pending", title: "Not made yet" }) : null,
    // The path only when it says more than the name.
    r.editPath && r.editPath !== e.name ? h("span", { class: "fpath", text: r.editPath, title: e.path }) : null,
  );

  const lines = h("div", { class: "lines" },
    ...e.lines.map((l) => h("div", { class: `l ${l.kind}` },
      h("span", { class: "n", text: l.num == null ? "" : String(l.num) }),
      h("span", { class: "s", text: SIGN[l.kind] }),
      h("span", { class: "t", text: l.text }),
    )),
  );

  const notes: string[] = [];
  if (e.status === "pending") notes.push(r.attention ? "Waiting for your permission." : "About to change.");
  if (e.status === "failed") notes.push("This change did not go through.");
  if (e.created) notes.push("New file.");
  if (e.replaceAll) notes.push("Every occurrence in the file.");
  if (!e.numbered) notes.push("Line numbers once the change is made.");
  if (e.cut) notes.push("Only part of the change is shown.");

  parts.code.append(
    ...present(
      r.steps.length ? steps : null,
      h("div", { class: `file ${e.status}` }, tab, lines),
      notes.length ? h("div", { class: "code-note", text: notes.join(" ") }) : null,
    ),
  );
}

function present(...nodes: (Node | null)[]): Node[] {
  return nodes.filter((n): n is Node => n !== null);
}

function renderSessions(list: SessionRow[]) {
  const seen = new Set<string>();
  list.forEach((r, i) => {
    seen.add(r.id);
    let parts = rows.get(r.id);
    if (!parts) {
      parts = makeRow(r.id);
      rows.set(r.id, parts);
    }
    parts.row.dataset.state = r.state;
    parts.row.classList.toggle("attention", r.attention);
    parts.row.classList.toggle("dim", r.dim);
    parts.row.classList.toggle("unseen", r.unseen);
    parts.project.textContent = r.project;
    clear(parts.chips);
    if (r.bypass) parts.chips.append(h("span", { class: "chip", text: "bypass" }));
    const where = [r.host, r.shell].filter(Boolean).join(" · ");
    parts.state.textContent = where ? `${r.label} · ${where}` : r.label;
    parts.detail.textContent = r.detail;
    parts.jump.title = r.jumpLabel;
    parts.jump.setAttribute("aria-label", r.jumpLabel);
    renderToggle(parts, r);
    renderCode(parts, r);
    // Keep the order of the list without re-adding rows that are in place.
    if (sessionList.children[i] !== parts.el) sessionList.insertBefore(parts.el, sessionList.children[i] ?? null);
  });
  for (const [id, parts] of rows) {
    if (!seen.has(id)) {
      parts.el.remove();
      rows.delete(id);
      expanded.delete(id);
    }
  }
  empty.style.display = list.length === 0 ? "" : "none";
}

let servicesKey = "";

function renderServices(list: ServiceRow[]) {
  const key = JSON.stringify(list);
  if (key === servicesKey) return;
  servicesKey = key;
  clear(serviceList);
  servicesTitle.style.display = list.length === 0 ? "none" : "";
  for (const s of list) {
    serviceList.append(
      h("div", {
        class: `row service ${s.status}`, role: "listitem", tabindex: "0",
        // A plugin opens its own page; a built-in opens its card on the island.
        onclick: () => {
          if (s.link) {
            void Bridge.openUrl(s.link);
            void Bridge.flyoutHide();
          } else open("overview", s.id);
        },
      },
        h("i", { class: "status", style: `background:${s.color}` }),
        h("div", { class: "text" }, h("div", { class: "line" }, h("span", { class: "project", text: s.name })), h("div", { class: "state", text: s.detail })),
      ),
    );
  }
}

let lastSnapshot: ShellSnapshot | null = null;

function render(s: ShellSnapshot) {
  lastSnapshot = s;
  headline.textContent = s.headline;
  clear(pauseBtn);
  pauseBtn.append(svg(s.paused ? PLAY : PAUSE, 16));
  pauseBtn.title = s.paused ? "Resume" : "Pause";
  document.body.classList.toggle("paused", s.paused);
  mochi.state = s.botState;
  syncMiniBotStates([mochi]);
  renderApproval(s.approval);
  renderSessions(s.sessions);
  renderServices(s.services);
}

// ── Mochi ─────────────────────────────────────────────────────────────────────

let last = performance.now();
function frame(now: number) {
  tickMiniBots(Math.min(0.05, (now - last) / 1000));
  last = now;
  if (document.visibilityState === "visible") requestAnimationFrame(frame);
}
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    last = performance.now();
    requestAnimationFrame(frame);
  }
});
requestAnimationFrame(frame);

// ── Boot ──────────────────────────────────────────────────────────────────────

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") void Bridge.flyoutHide();
});

void onEvent<ShellSnapshot>("shell-snapshot", render);
void Bridge.shellSnapshot().then((s) => {
  if (s) render(s);
});
