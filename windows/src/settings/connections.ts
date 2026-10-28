import { Bridge, type AgentConnection } from "../core/bridge";
import type { ProviderId } from "../core/providers";
import { h, clear } from "../views/dom";

/** Connections use the official CLIs' account stores; this view never handles credentials. */
export function connectionsSection(current: () => ProviderId, select: (id: ProviderId) => Promise<void>): HTMLElement {
  const list = h("div");
  const feedback = h("div", { class: "hint", role: "status", "aria-live": "polite" });
  const project = h("input", {
    type: "text", placeholder: "Project folder", "aria-label": "Project folder",
    spellcheck: "false", style: "flex:1;min-width:0",
    value: sessionStorage.getItem("agent-project") ?? "",
  }) as HTMLInputElement;
  project.addEventListener("input", () => sessionStorage.setItem("agent-project", project.value));
  let loading = false;
  let connections: AgentConnection[] = [];

  function draw() {
    clear(list);
    for (const agent of connections) {
      const message = h("div", { class: "hint", text: agent.message });
      const login = h("button", { text: "Sign in", disabled: !agent.installed });
      login.addEventListener("click", () => void action(login, async () => {
        await Bridge.agentOpen(agent.id, true, null);
        feedback.textContent = "Finish signing in in the agent window, then select Refresh connections.";
      }));
      const chat = h("button", {
        class: "primary", disabled: !agent.signedIn,
        text: current() === agent.id ? "Selected for chat" : "Use for chat",
      });
      chat.addEventListener("click", () => void action(chat, async () => {
        await select(agent.id);
        feedback.textContent = `${agent.name} is selected. Open Chat from the tray to start.`;
        draw();
      }));
      const open = h("button", { text: "Open project", disabled: !agent.signedIn });
      open.addEventListener("click", () => void action(open, async () => {
        await Bridge.agentOpen(agent.id, false, project.value.trim());
        feedback.textContent = `${agent.name} is opening in your project.`;
      }));
      const row = h("div", { class: "row" }, h("label", { text: agent.name }), chat, open);
      if (!agent.signedIn) row.append(login);
      list.append(row, message);
    }
  }

  async function action(button: HTMLElement, work: () => Promise<void>) {
    (button as HTMLButtonElement).disabled = true;
    feedback.textContent = "";
    try { await work(); }
    catch (error) { feedback.textContent = String(error).replace(/^Error:\s*/, ""); }
    finally { (button as HTMLButtonElement).disabled = false; }
  }

  const refresh = h("button", { text: "Refresh connections" });
  async function load() {
    if (loading) return;
    loading = true;
    (refresh as HTMLButtonElement).disabled = true;
    feedback.textContent = "Checking the installed agents…";
    try { connections = await Bridge.agentConnections(); feedback.textContent = ""; draw(); }
    catch (error) { feedback.textContent = String(error).replace(/^Error:\s*/, ""); }
    finally { loading = false; (refresh as HTMLButtonElement).disabled = false; }
  }
  refresh.addEventListener("click", () => void load());
  void load();
  return h("section", {},
    h("h2", {}, h("span", { text: "Claude Code and Codex" })),
    h("div", { class: "hint", text: "Use your existing Claude or ChatGPT sign-in. No Coucou account or API key is needed. Your plan's usage limits apply." }),
    list,
    h("div", { class: "row" }, h("label", { text: "Work in" }), project),
    h("div", { class: "hint", text: "Open project starts the full coding agent in its own terminal, with your skills, tools and permission prompts. Chat answers questions using the same sign-in." }),
    h("div", { class: "row" }, refresh), feedback,
    h("div", { class: "hint" },
      h("button", { text: "Install Claude Code", onclick: () => void Bridge.openUrl("https://code.claude.com/docs/en/setup") }),
      document.createTextNode(" · "),
      h("button", { text: "Install Codex", onclick: () => void Bridge.openUrl("https://developers.openai.com/codex/cli") }),
    ),
  );
}
