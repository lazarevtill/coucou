import type { Settings } from "../core/state";
import { h } from "../views/dom";
import { t } from "../i18n/i18n";

/** User-selected launchers for a coding session's project folder. */
export function projectsSection(settings: Settings, save: () => Promise<void>): HTMLElement {
  const status = h("div", { class: "hint", role: "status" });
  function choice<K extends "editor" | "terminal">(key: K, label: string, options: [Settings[K], string][]) {
    const select = h("select", { "aria-label": label });
    for (const [value, text] of options) select.append(h("option", { value, text }));
    select.value = settings[key];
    select.addEventListener("change", async () => {
      const next = options.find(([value]) => value === select.value)?.[0];
      if (next === undefined) return;
      select.disabled = true;
      const previous = settings[key];
      settings[key] = next;
      try { await save(); status.textContent = ""; }
      catch (error) {
        settings[key] = previous;
        select.value = previous;
        status.textContent = String(error);
      }
      finally { select.disabled = false; }
    });
    return h("div", { class: "row" }, h("span", { text: label }), select);
  }
  return h("section", {}, h("h2", { text: t("Open projects in") }),
    choice("editor", t("Editor"), [["vscode", "VS Code"], ["cursor", "Cursor"], ["none", t("None")]]),
    choice("terminal", t("Terminal"), [["windowsTerminal", "Windows Terminal"], ["shell", "PowerShell"], ["none", t("None")]]),
    status);
}
