import { Bridge, type McpStatus } from "../core/bridge";
import { t } from "../i18n/i18n";
import { h } from "../views/dom";

/** Arguments are quoted for PowerShell on Windows and POSIX shells on Linux. */
export function mcpSetup(status: McpStatus) {
  const command = `'${status.executable.replaceAll("'", status.windows ? "''" : "'\"'\"'")}'`;
  return {
    claude: `claude mcp add --scope user --transport stdio coucou-notes -- ${command} mcp`,
    codex: `codex mcp add coucou-notes -- ${command} mcp`,
    desktop: JSON.stringify({ mcpServers: { "coucou-notes": { command: status.executable, args: ["mcp"] } } }, null, 2),
  };
}

export function mcpSection(): HTMLElement {
  let status: McpStatus | null = null;
  let pending = false;
  const feedback = h("div", { class: "hint", role: "status", "aria-live": "polite" });
  const toggle = h("button", { class: "switch", role: "switch", "aria-checked": "false",
    "aria-label": t("Allow agents to read saved calls"), disabled: true });
  const refresh = h("button", { text: t("Refresh") });
  const client = h("select", { "aria-label": t("MCP client") },
    h("option", { value: "claude", text: t("Claude Code") }),
    h("option", { value: "codex", text: t("Codex") }),
    h("option", { value: "desktop", text: t("Claude Desktop") }),
    h("option", { value: "codex-desktop", text: t("Codex desktop / ChatGPT desktop") }));
  const code = h("textarea", { class: "diff", readonly: true, rows: 5, spellcheck: "false", "aria-label": t("MCP setup"),
    style: "width:100%;box-sizing:border-box;resize:vertical" });
  const instruction = h("div", { class: "hint" });
  const copy = h("button", { text: t("Copy"), disabled: true });

  function draw() {
    toggle.classList.toggle("on", status?.enabled === true);
    toggle.setAttribute("aria-checked", String(status?.enabled === true));
    toggle.disabled = pending || status === null;
    refresh.disabled = pending;
    copy.disabled = pending || status === null;
    const key = client.value === "codex" || client.value === "codex-desktop" ? "codex" : client.value === "desktop" ? "desktop" : "claude";
    code.value = status ? mcpSetup(status)[key] : "";
    instruction.textContent = key === "desktop"
      ? t("Merge this entry into Claude Desktop's Developer > Edit Config. Keep existing servers, then restart Claude.")
      : t("Review and run this command in your terminal (PowerShell on Windows), then restart the agent.");
  }

  async function load(change?: boolean) {
    if (pending) return;
    pending = true;
    feedback.textContent = "";
    draw();
    try {
      status = change === undefined ? await Bridge.mcpStatus() : await Bridge.mcpSetEnabled(change);
    } catch (error) {
      feedback.textContent = String(error).replace(/^Error:\s*/, "");
      // A failed save keeps the last observed value; an initial read failure
      // never presents a usable permission switch or an invented executable.
    } finally { pending = false; draw(); }
  }
  toggle.addEventListener("click", () => { if (status) void load(!status.enabled); });
  refresh.addEventListener("click", () => void load());
  client.addEventListener("change", draw);
  copy.addEventListener("click", async () => {
    if (!status || pending) return;
    try { await navigator.clipboard.writeText(code.value); feedback.textContent = t("Copied"); }
    catch { feedback.textContent = t("Copy failed. Select and copy the setup text manually."); }
  });
  void load();
  return h("section", {},
    h("h2", {}, h("span", { text: t("Local agents (MCP)") })),
    h("div", { class: "hint", text: t("Allow connected local agents to search all saved calls and read transcripts, summaries and action items. Your agent may send this text to its AI provider.") }),
    h("div", { class: "row" }, h("label", { text: t("Allow agents to read saved calls") }), toggle),
    h("div", { class: "hint", text: t("Read-only and off by default. Turning this off blocks further reads, but cannot erase text already shared with an agent.") }),
    h("div", { class: "row" }, client, copy, refresh), instruction, code,
    h("div", { class: "hint", text: t("ChatGPT desktop uses the Codex configuration on this computer. Browser chats require a remote connection.") }),
    feedback,
  );
}
