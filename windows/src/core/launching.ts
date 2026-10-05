// What the island says while an editor or a terminal is being opened. Pure, so
// it is tested directly.
//
// An editor can take seconds to come up. Without a word on screen that looked
// like nothing happened, so it got clicked again — and each click opened another
// window. The app now refuses the same launch while it starts (open.rs); this is
// the island's side: say it is on its way, and for how long.

import type { OpenResult } from "./bridge";

/** The note after a click on Open…; `what` names the editor or terminal. */
export function openNote(r: OpenResult | null, what: string): string | null {
  if (!r) return null;
  if (r.via === "starting") return `${what} is already starting…`;
  if (r.fellBack) return `${r.error ?? `${what} could not be opened.`} The folder opened in File Explorer instead.`;
  if (r.via === "none") return r.error;
  return `Opening ${what}…`;
}

/** Until when the button should say the launch is on its way. */
export function startedUntil(r: OpenResult | null, now: number): number | null {
  return r?.startingMs != null ? now + r.startingMs : null;
}

export function stillStarting(until: number | null, now: number): boolean {
  return until != null && now < until;
}

/** What Open… starts, as Settings → Open projects in chose it; null when switched off. */
export function launcherName(target: "editor" | "terminal", s: { editor: string; terminal: string }): string | null {
  const names: Record<string, string> =
    target === "editor"
      ? { cursor: "Cursor", vscode: "VS Code" }
      : { windowsTerminal: "Windows Terminal", shell: "PowerShell" };
  return names[target === "editor" ? s.editor : s.terminal] ?? null;
}
