import { Bridge } from "../core/bridge";
import { State } from "../core/state";
import { language, t } from "../i18n/i18n";
import { clear, h } from "./dom";

/** Session selection changes only the jump target, never a pending approval. */
export function sessionConnections() {
  const el = h("div", { class: "session-connections", style: "display:flex;flex-direction:column;gap:6px" });
  let key = "";
  let selected = "";
  return {
    el,
    sync(agent: "claude" | "codex" | null): boolean {
      const sessions = agent ? State.sessions.list(agent).sort((a, b) => a.id.localeCompare(b.id)) : [];
      const next = JSON.stringify([language(), State.settings.editor, State.settings.terminal,
        sessions.map(s => [s.id, s.project, s.cwd, s.host?.label])]);
      el.hidden = sessions.length === 0;
      if (next === key) return sessions.length > 0;
      key = next;
      clear(el);
      if (!sessions.length) return false;
      const select = h("select", { "aria-label": agent === "codex" ? "Codex" : "Claude Code", style: "width:100%;min-width:0" });
      for (const session of sessions) select.append(h("option", {
        value: session.id, text: `${session.project}${session.host ? ` · ${session.host.label}` : ""}`,
      }));
      selected = sessions.some(s => s.id === selected) ? selected : sessions[0].id;
      select.value = selected;
      const status = h("span", { role: "status", style: "font-size:11px;overflow-wrap:anywhere" });
      select.addEventListener("change", () => { selected = select.value; status.textContent = ""; });
      const buttons = h("div", { style: "display:flex;gap:6px;flex-wrap:wrap" });
      function button(text: string, run: () => Promise<void>) {
        const el = h("button", { class: "btn secondary", text });
        el.addEventListener("click", async () => {
          el.disabled = true;
          status.textContent = "";
          try { await run(); } catch (error) { status.textContent = String(error); }
          finally { el.disabled = false; }
        });
        buttons.append(el);
      }
      button(t("Open"), async () => {
        const result = await Bridge.focusSession(selected);
        if (!result || result.outcome === "noWindow" || result.outcome === "unknownSession") status.textContent = t("Not available");
        else if (result.outcome === "flashed") status.textContent = t("Check the taskbar for the session window.");
        else if (result.outcome === "focusedUnsure") status.textContent = t("Check the selected session in the opened window.");
      });
      const launch = async (target: "editor" | "terminal") => {
        const session = State.sessions.get(selected);
        if (!session) { status.textContent = t("Not available"); return; }
        const result = await Bridge.openProject(target, session.cwd);
        if (!result) status.textContent = t("Not available");
        else if (result.result.error) status.textContent = result.result.error;
      };
      if (State.settings.editor !== "none") button(t("Open in {0}", { 0: State.settings.editor === "cursor" ? "Cursor" : "VS Code" }), () => launch("editor"));
      if (State.settings.terminal !== "none") button(t("Open terminal"), () => launch("terminal"));
      el.append(select, buttons, status);
      return true;
    },
  };
}
