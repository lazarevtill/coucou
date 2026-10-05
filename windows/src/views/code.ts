// A change to a file, drawn the same way in the tray flyout and on the island:
// a tab with the file, then its lines, numbered, removed in red and added in
// green. Each page styles it in its own look. Text only — never HTML: the lines
// are a session's code.

import type { EditLine, FileEdit, LineKind } from "../core/edits";
import { h } from "./dom";

const SIGN: Record<LineKind, string> = { ctx: " ", del: "−", add: "+", gap: "⋯" };

/** The file's tab: its kind, its name, a dot while the change is not made yet, and its path. */
export function fileTab(e: FileEdit, path: string | null): HTMLElement {
  return h("div", { class: "file-tab" },
    h("span", { class: "badge", text: e.badge }),
    h("span", { class: "fname", text: e.name }),
    e.status === "pending" ? h("i", { class: "pending", title: "Not made yet" }) : null,
    // The path only when it says more than the name.
    path && path !== e.name ? h("span", { class: "fpath", text: path, title: e.path }) : null,
  );
}

function line(l: EditLine): HTMLElement {
  return h("div", { class: `l ${l.kind}` },
    h("span", { class: "n", text: l.num == null ? "" : String(l.num) }),
    h("span", { class: "s", text: SIGN[l.kind] }),
    h("span", { class: "t", text: l.text }),
  );
}

/** The change's lines. */
export function codeLines(e: FileEdit): HTMLElement {
  return h("div", { class: "lines" }, ...e.lines.map(line));
}
