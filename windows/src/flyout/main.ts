// The tray flyout. It owns nothing: it draws the snapshot the island publishes
// and sends what is clicked back to it (see island/shellLink.ts). Jumping to a
// session's window is the one thing it does itself, straight through Rust.
//
// Rows are updated in place rather than rebuilt: snapshots arrive while a
// session works, and a button replaced between mouse-down and mouse-up would
// swallow the click.

import "./flyout.css";
import { Bridge, onEvent, type ShellAction } from "../core/bridge";
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
  el: HTMLElement;
  project: HTMLElement;
  chips: HTMLElement;
  state: HTMLElement;
  detail: HTMLElement;
  jump: HTMLButtonElement;
}

const rows = new Map<string, RowParts>();

function makeRow(id: string): RowParts {
  const project = h("span", { class: "project" });
  const chips = h("span", { class: "chips" });
  const state = h("div", { class: "state" });
  const detail = h("div", { class: "detail" });
  const jumpBtn = h("button", { class: "jump", onclick: (e: Event) => { e.stopPropagation(); void jump(id); } }, svg(ICONS.arrowUpRight, 14));
  const el = h("div", { class: "row", role: "listitem", tabindex: "0", onclick: () => void jump(id) },
    h("i", { class: "status" }),
    h("div", { class: "text" }, h("div", { class: "line" }, project, chips), state, detail),
    jumpBtn,
  );
  el.addEventListener("keydown", (e) => {
    if (e.key === "Enter") void jump(id);
  });
  return { el, project, chips, state, detail, jump: jumpBtn };
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
    parts.el.dataset.state = r.state;
    parts.el.classList.toggle("attention", r.attention);
    parts.el.classList.toggle("dim", r.dim);
    parts.el.classList.toggle("unseen", r.unseen);
    parts.project.textContent = r.project;
    clear(parts.chips);
    if (r.bypass) parts.chips.append(h("span", { class: "chip", text: "bypass" }));
    const where = [r.host, r.shell].filter(Boolean).join(" · ");
    parts.state.textContent = where ? `${r.label} · ${where}` : r.label;
    parts.detail.textContent = r.detail;
    parts.jump.title = r.jumpLabel;
    parts.jump.setAttribute("aria-label", r.jumpLabel);
    // Keep the order of the list without re-adding rows that are in place.
    if (sessionList.children[i] !== parts.el) sessionList.insertBefore(parts.el, sessionList.children[i] ?? null);
  });
  for (const [id, parts] of rows) {
    if (!seen.has(id)) {
      parts.el.remove();
      rows.delete(id);
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
      h("div", { class: `row service ${s.status}`, role: "listitem", tabindex: "0", onclick: () => open("overview", s.id) },
        h("i", { class: "status", style: `background:${s.color}` }),
        h("div", { class: "text" }, h("div", { class: "line" }, h("span", { class: "project", text: s.name })), h("div", { class: "state", text: s.detail })),
      ),
    );
  }
}

function render(s: ShellSnapshot) {
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
