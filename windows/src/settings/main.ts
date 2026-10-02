// Settings window — the place where anything that writes to disk is confirmed.
// Stage 2 covers the Claude Code hooks and the general preferences; API keys and
// integrations land here too in a later stage.

import "./settings.css";
import { Bridge, onEvent, type HookStatus, type LaunchInfo, type PluginInfo } from "../core/bridge";
import { DEFAULT_SETTINGS, type Settings } from "../core/state";
import { h, clear } from "../views/dom";
import { builtinsOn, isOn, MAX_BUILTINS, withOn } from "../core/plugins";

let settings: Settings = { ...DEFAULT_SETTINGS };
let version = "";

const root = document.getElementById("settings-root")!;

async function save() {
  await Bridge.saveSettings(settings);
}

// ── Reusable bits ─────────────────────────────────────────────────────────────

function toggle(on: boolean, onChange: (v: boolean) => void): HTMLElement {
  const el = h("button", { class: on ? "switch on" : "switch", "aria-pressed": on });
  el.addEventListener("click", () => {
    const next = !el.classList.contains("on");
    el.classList.toggle("on", next);
    onChange(next);
  });
  return el;
}

function statusDot(ok: boolean): HTMLElement {
  return h("i", { class: "dot", style: `background:${ok ? "#22c55e" : "#f4505e"}` });
}

function renderDiff(text: string): HTMLElement {
  const box = h("div", { class: "diff" });
  for (const line of text.split("\n")) {
    const cls = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "ctx";
    box.append(h("div", { class: cls, text: line }));
  }
  return box;
}

// ── Claude Code section ───────────────────────────────────────────────────────

function claudeSection(status: HookStatus): HTMLElement {
  const body = h("div", { style: "display:flex;flex-direction:column;gap:12px" });
  const section = h(
    "section",
    {},
    h("h2", {}, statusDot(status.installed), h("span", { text: "Claude Code" })),
    body,
  );

  const rebuild = async () => {
    const fresh = await Bridge.hooksStatus();
    if (fresh) Object.assign(status, fresh);
    clear(body);
    draw();
    const head = section.querySelector("h2")!;
    clear(head);
    head.append(statusDot(status.installed), h("span", { text: "Claude Code" }));
  };

  function draw() {
    body.append(
      h("div", {
        class: "hint",
        text: status.installed
          ? "Coucou is hooked into your Claude Code sessions. Tool calls, questions and permission requests show up in the island, and you can answer them there."
          : "Install the hooks to see your Claude Code sessions in the island and approve permissions without leaving what you are doing.",
      }),
      h("div", { class: "row" },
        h("label", { text: "settings.json" }),
        h("span", { class: "path", text: status.settingsPath }),
      ),
      h("div", { class: "row" },
        h("label", { text: "Relay" }),
        h("span", { class: "path", text: status.hookPath }),
        statusDot(status.hookReady),
      ),
    );

    if (!status.hookReady) {
      body.append(h("div", {
        class: "notice warn",
        text: "coucou-hook.exe is not in place yet. Restart Coucou; if it still fails, build it with `cargo build -p coucou-hook`.",
      }));
    }

    const actions = h("div", { class: "row" });
    const install = h("button", {
      class: "primary",
      text: status.installed ? "Reinstall hooks…" : "Install hooks…",
      onclick: () => showPreview(true),
    });
    // Writing hook commands that point at a relay which isn't there would give
    // every Claude Code session a broken hook and nothing to show for it.
    if (!status.hookReady) {
      install.disabled = true;
      install.title = "The relay isn't installed yet.";
    }
    actions.append(install);
    if (status.installed) {
      actions.append(h("button", {
        class: "danger",
        text: "Uninstall hooks…",
        onclick: () => showPreview(false),
      }));
    }
    body.append(actions);
  }

  async function showPreview(install: boolean) {
    let preview;
    try {
      preview = await Bridge.hooksPreview(install);
    } catch (err) {
      // An unreadable or invalid settings.json stops here rather than being
      // treated as empty and written over.
      clear(body);
      body.append(
        h("div", { class: "notice err", text: String(err).replace(/^Error:\s*/, "") }),
        h("div", { class: "row" }, h("button", {
          text: "Back",
          onclick: () => { clear(body); draw(); },
        })),
      );
      return;
    }
    if (!preview) return;
    clear(body);
    body.append(
      h("div", {
        class: "hint",
        text: install
          ? "This is exactly what will change in your settings.json. Your own hooks are left untouched."
          : "This removes Coucou's entries only. Your own hooks are left untouched.",
      }),
      renderDiff(preview.diff),
      h("div", { class: "row" },
        h("span", { class: "path", text: `Backup → ${preview.backup}` }),
      ),
    );
    const confirm = h("button", {
      class: install ? "primary" : "danger",
      text: install ? "Back up and write" : "Back up and remove",
    });
    confirm.addEventListener("click", async () => {
      confirm.disabled = true;
      try {
        const backup = await Bridge.hooksApply(install, preview.fingerprint);
        clear(body);
        body.append(h("div", {
          class: "notice ok",
          text: `Done. Previous settings saved as ${backup}. Open a new Claude Code session to pick the hooks up.`,
        }));
        window.setTimeout(() => void rebuild(), 2600);
      } catch (err) {
        confirm.disabled = false;
        body.append(h("div", { class: "notice err", text: `Could not write: ${String(err)}` }));
      }
    });
    body.append(h("div", { class: "row" }, confirm, h("button", {
      text: "Cancel",
      onclick: () => { clear(body); draw(); },
    })));
  }

  draw();
  return section;
}

// ── Claude API section ────────────────────────────────────────────────────────

const MODELS: [string, string][] = [
  ["claude-opus-5", "Claude Opus 5"],
  ["claude-sonnet-5", "Claude Sonnet 5"],
  ["claude-haiku-4-5", "Claude Haiku 4.5"],
];

function apiSection(hasKey: boolean): HTMLElement {
  const dot = statusDot(hasKey);
  const state = h("span", { class: "hint", text: hasKey ? "Key saved in the Windows Credential Manager." : "No key yet — the chat needs one." });

  const field = h("input", {
    type: "password",
    placeholder: hasKey ? "••••••••••••  (stored)" : "sk-ant-...",
    style: "flex:1 1 auto;min-width:0",
    autocomplete: "off",
    spellcheck: "false",
  }) as HTMLInputElement;

  const saveBtn = h("button", { class: "primary", text: "Save key" });
  const clearBtn = h("button", { class: "danger", text: "Remove" });
  const feedback = h("div", {});

  async function refresh() {
    const present = (await Bridge.secretPresent("anthropic-api-key")) ?? false;
    dot.style.background = present ? "#22c55e" : "#f4505e";
    state.textContent = present
      ? "Key saved in the Windows Credential Manager."
      : "No key yet — the chat needs one.";
    field.placeholder = present ? "••••••••••••  (stored)" : "sk-ant-...";
    clearBtn.style.display = present ? "" : "none";
  }

  saveBtn.addEventListener("click", async () => {
    const value = field.value.trim();
    if (!value) return;
    clear(feedback);
    try {
      await Bridge.secretSet("anthropic-api-key", value);
      field.value = "";
      feedback.append(h("div", { class: "notice ok", text: "Saved. It never touches disk." }));
      await refresh();
    } catch (err) {
      feedback.append(h("div", { class: "notice err", text: `Could not save: ${String(err)}` }));
    }
  });

  clearBtn.addEventListener("click", async () => {
    clear(feedback);
    try {
      await Bridge.secretClear("anthropic-api-key");
      feedback.append(h("div", { class: "notice ok", text: "Key removed." }));
      await refresh();
    } catch (err) {
      feedback.append(h("div", { class: "notice err", text: `Could not remove: ${String(err)}` }));
    }
  });

  const model = h("select", {}) as HTMLSelectElement;
  for (const [id, label] of MODELS) model.append(h("option", { value: id, text: label }));
  if (!MODELS.some(([id]) => id === settings.model)) {
    model.append(h("option", { value: settings.model, text: settings.model }));
  }
  model.value = settings.model;
  model.addEventListener("change", () => {
    settings.model = model.value;
    void save();
  });

  clearBtn.style.display = hasKey ? "" : "none";

  return h(
    "section",
    {},
    h("h2", {}, dot, h("span", { text: "Claude" })),
    state,
    h("div", { class: "row" }, h("label", { text: "API key" }), field, saveBtn, clearBtn),
    h("div", { class: "row" }, h("label", { text: "Model" }), model),
    feedback,
  );
}

// ── Chat source section ───────────────────────────────────────────────────────

function chatSourceSection(serverKey: boolean): HTMLElement {
  const provider = h("select", {}) as HTMLSelectElement;
  provider.append(
    h("option", { value: "anthropic", text: "Claude (Anthropic API)" }),
    h("option", { value: "llmServer", text: "Your model server (llama.cpp, LM Studio, Ollama…)" }),
  );
  provider.value = settings.chatProvider;

  const url = h("input", {
    type: "text", value: settings.llmServerUrl, spellcheck: "false", autocomplete: "off",
    placeholder: "http://127.0.0.1:8080/v1", style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  const check = h("button", { text: "Check" });
  const status = h("div", { class: "hint" });
  const model = h("select", {}) as HTMLSelectElement;

  function fillModels(ids: string[]) {
    clear(model);
    model.append(h("option", { value: "", text: ids.length ? `First listed (${ids[0]})` : "First listed by the server" }));
    for (const id of ids) model.append(h("option", { value: id, text: id }));
    if (settings.llmServerModel && !ids.includes(settings.llmServerModel)) {
      model.append(h("option", { value: settings.llmServerModel, text: settings.llmServerModel }));
    }
    model.value = settings.llmServerModel;
  }
  fillModels([]);

  async function probe() {
    status.textContent = "Checking…";
    try {
      const ids = await Bridge.llmServerModels(url.value.trim());
      status.textContent = ids.length ? `Connected — ${ids.length} model${ids.length === 1 ? "" : "s"}.` : "Connected, but the server lists no model.";
      fillModels(ids);
      return true;
    } catch (err) {
      status.textContent = String(err).replace(/^Error:\s*/, "");
      return false;
    }
  }

  check.addEventListener("click", async () => {
    // Only an address that answers is kept.
    if (await probe()) {
      settings.llmServerUrl = url.value.trim();
      void save();
    }
  });
  model.addEventListener("change", () => {
    settings.llmServerModel = model.value;
    void save();
  });

  const keyField = h("input", {
    type: "password", autocomplete: "off", spellcheck: "false",
    placeholder: serverKey ? "••••••••  (stored)" : "Only if the server asks for one",
    style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  const keySave = h("button", { text: "Save" });
  keySave.addEventListener("click", async () => {
    try {
      await Bridge.secretSet("llm-server-api-key", keyField.value.trim());
      keyField.placeholder = keyField.value.trim() ? "••••••••  (stored)" : "Only if the server asks for one";
      keyField.value = "";
    } catch (err) {
      status.textContent = `Could not save the key: ${String(err)}`;
    }
  });

  const server = h("div", { style: "display:flex;flex-direction:column;gap:10px" },
    h("div", {
      class: "hint",
      text: "Anything that speaks the OpenAI chat API. Plain http only for this computer and your local network; the conversation goes nowhere else. Text files only — PDFs and images need Claude.",
    }),
    h("div", { class: "row" }, h("label", { text: "Address" }), url, check),
    status,
    h("div", { class: "row" }, h("label", { text: "Model" }), model),
    h("div", { class: "row" }, h("label", { text: "API key" }), keyField, keySave),
  );

  const sync = () => {
    server.style.display = provider.value === "llmServer" ? "" : "none";
  };
  provider.addEventListener("change", () => {
    settings.chatProvider = provider.value as Settings["chatProvider"];
    void save();
    void Bridge.chatReset(); // a conversation does not carry over to another source
    sync();
  });
  sync();
  if (settings.chatProvider === "llmServer") void probe();

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Chat" })),
    h("div", { class: "row" }, h("label", { text: "Answers from" }), provider),
    server,
  );
}

// ── Integrations section ──────────────────────────────────────────────────────

interface IntegrationDef {
  id: string;
  name: string;
  color: string;
  /** Credential Manager keys, in the order they are shown. */
  fields: { key: string; label: string; placeholder: string; secret: boolean }[];
}

const INTEGRATIONS: IntegrationDef[] = [
  { id: "integration_stripe", name: "Stripe", color: "#0570DE",
    fields: [{ key: "stripe-api-key", label: "Secret key", placeholder: "sk_live_…", secret: true }] },
  { id: "integration_github", name: "GitHub", color: "#F4505E",
    fields: [{ key: "github-token", label: "Token", placeholder: "ghp_…", secret: true }] },
  { id: "integration_vercel", name: "Vercel", color: "#7C5CFF",
    fields: [{ key: "vercel-token", label: "Token", placeholder: "…", secret: true }] },
  { id: "integration_n8n", name: "n8n", color: "#F29B38",
    fields: [
      { key: "n8n-url", label: "Instance URL", placeholder: "https://n8n.example.com", secret: false },
      { key: "n8n-api-key", label: "API key", placeholder: "…", secret: true },
    ] },
  { id: "integration_resend", name: "Resend", color: "#22C55E",
    fields: [{ key: "resend-api-key", label: "API key", placeholder: "re_…", secret: true }] },
  { id: "integration_notion", name: "Notion", color: "#8C8C8C",
    fields: [{ key: "notion-api-key", label: "Integration token", placeholder: "ntn_…", secret: true }] },
  { id: "integration_calcom", name: "Cal.com", color: "#C9956A",
    fields: [{ key: "calcom-api-key", label: "API key", placeholder: "cal_…", secret: true }] },
];



function integrationsSection(present: Record<string, boolean>): HTMLElement {
  const note = h("div", { class: "hint" });
  const list = h("div", { style: "display:flex;flex-direction:column;gap:14px" });

  function updateNote() {
    const used = builtinsOn(settings.plugins).length;
    note.textContent = `Pick up to ${MAX_BUILTINS} pills to show next to Mochi — ${used}/${MAX_BUILTINS} in use. Keys are stored in the Windows Credential Manager, never on disk.`;
  }

  for (const def of INTEGRATIONS) {
    const active = isOn(settings.plugins, def.id);
    const sw = h("button", { class: active ? "switch on" : "switch" });
    sw.addEventListener("click", () => {
      const on = isOn(settings.plugins, def.id);
      if (!on && builtinsOn(settings.plugins).length >= MAX_BUILTINS) return;
      settings.plugins = withOn(settings.plugins, def.id, !on);
      sw.classList.toggle("on", !on);
      updateNote();
      void save();
    });

    const rows = h("div", { style: "display:flex;flex-direction:column;gap:6px;flex:1 1 auto;min-width:0" });
    for (const field of def.fields) {
      const input = h("input", {
        type: field.secret ? "password" : "text",
        placeholder: present[field.key] ? "••••••••  (stored)" : field.placeholder,
        autocomplete: "off",
        spellcheck: "false",
        style: "flex:1 1 auto;min-width:0",
      }) as HTMLInputElement;
      const saveBtn = h("button", { text: "Save" });
      const dotEl = statusDot(present[field.key] ?? false);
      saveBtn.addEventListener("click", async () => {
        const value = input.value.trim();
        try {
          await Bridge.secretSet(field.key, value);
          present[field.key] = value.length > 0;
          input.value = "";
          input.placeholder = value ? "••••••••  (stored)" : field.placeholder;
          dotEl.style.background = value ? "#22c55e" : "#f4505e";
        } catch {
          dotEl.style.background = "#f5a524";
        }
      });
      rows.append(
        h("div", { class: "row" },
          h("label", { style: "min-width:104px", text: field.label }),
          input, saveBtn, dotEl,
        ),
      );
    }

    list.append(
      h("div", { style: "display:flex;gap:12px;align-items:flex-start" },
        h("div", { style: "display:flex;align-items:center;gap:8px;min-width:132px;padding-top:4px" },
          sw,
          h("i", { class: "dot", style: `background:${def.color}` }),
          h("span", { style: "font-size:12.5px", text: def.name }),
        ),
        rows,
      ),
    );
  }

  updateNote();
  return h("section", {}, h("h2", {}, h("span", { text: "Integrations" })), note, list);
}

// ── Open-in section ───────────────────────────────────────────────────────────

function openInSection(info: LaunchInfo | null): HTMLElement {
  /** "(not found)" only when the app could look and did not find it. */
  const found = (yes: boolean | undefined) => (info && !yes ? " (not found)" : "");
  const shell = info?.shell === "pwsh" ? "PowerShell 7" : info?.shell === "powershell" ? "Windows PowerShell" : null;

  const editor = h("select", {}) as HTMLSelectElement;
  editor.append(
    h("option", { value: "cursor", text: `Cursor${found(info?.cursor)}` }),
    h("option", { value: "vscode", text: `VS Code${found(info?.vscode)}` }),
    h("option", { value: "none", text: "None" }),
  );
  editor.value = settings.editor;
  editor.addEventListener("change", () => {
    settings.editor = editor.value as Settings["editor"];
    void save();
  });

  const terminal = h("select", {}) as HTMLSelectElement;
  terminal.append(
    h("option", { value: "windowsTerminal", text: `Windows Terminal${found(info?.windowsTerminal)}` }),
    h("option", { value: "shell", text: `${shell ?? "PowerShell"} window${found(shell !== null)}` }),
    h("option", { value: "none", text: "None" }),
  );
  terminal.value = settings.terminal;
  terminal.addEventListener("change", () => {
    settings.terminal = terminal.value as Settings["terminal"];
    void save();
  });

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Open projects in" })),
    h("div", {
      class: "hint",
      text: `What the island's buttons open. A missing one falls back to File Explorer.${shell ? ` New terminals start ${shell}.` : ""}`,
    }),
    h("div", { class: "row" }, h("label", { text: "Editor" }), editor),
    h("div", { class: "row" }, h("label", { text: "Terminal" }), terminal),
  );
}

// ── Plugins section ───────────────────────────────────────────────────────────

const KIND_LABEL: Record<string, string> = {
  http: "Reads a web address",
  "mcp-stdio": "Runs a program (MCP server)",
  "mcp-http": "Talks to an MCP server",
};

const APPROVAL_LABEL: Record<PluginInfo["approval"], [string, string]> = {
  on: ["On", "#22c55e"],
  off: ["Off", "#8e939c"],
  changed: ["Changed since you approved it", "#f5a524"],
  invalid: ["Can't be used", "#f4505e"],
};

function pluginCard(p: PluginInfo, rerender: () => void): HTMLElement {
  const [label, color] = APPROVAL_LABEL[p.approval];
  const card = h("div", { class: "plugin-card", style: "border:1px solid #2a2d33;border-radius:8px;padding:12px;display:flex;flex-direction:column;gap:8px" });
  card.append(
    h("div", { class: "row", style: "gap:8px" },
      h("i", { class: "dot", style: `background:${p.color ?? "#8c8c8c"}` }),
      h("strong", { text: p.name ?? p.id }),
      h("span", { class: "hint", text: [p.version && `v${p.version}`, p.kind && KIND_LABEL[p.kind]].filter(Boolean).join(" · ") }),
      h("span", { class: "grow" }),
      h("span", { style: `color:${color};font-size:12px`, text: label }),
    ),
  );
  const feedback = h("div", {});
  if (p.error) card.append(h("div", { class: "notice err", text: p.error }));
  if (p.approval === "invalid") return card;

  // What it does, in full, before anything is trusted.
  if (p.kind === "mcp-stdio") {
    card.append(
      h("div", { class: "hint", text: "Runs this program, with your permissions:" }),
      h("code", { class: "path", style: "white-space:pre-wrap;word-break:break-all", text: p.runs ?? "" }),
      h("div", { class: "notice warn", text: "A program can do anything you can do on this computer. Coucou starts it without a shell and with only the environment it needs, but cannot limit what it reaches." }),
    );
  } else {
    card.append(h("div", { class: "row" }, h("label", { text: "Reads" }), h("span", { class: "path", text: p.runs ?? "" })));
  }
  if (p.kind !== "mcp-stdio") {
    card.append(h("div", { class: "row" }, h("label", { text: "May connect to" }), h("span", { class: "path", text: p.hosts.join(", ") || "nothing" })));
  }
  if (p.allowPrivateNetwork) card.append(h("div", { class: "hint", text: "It may reach your local network." }));
  if (p.pollTool) {
    card.append(h("div", { class: "hint", text: `Every ${Math.round((p.pollSecs ?? 300) / 60)} min it calls ${p.pollTool}, which its manifest marks read-only.` }));
  }
  if (p.tools.length) {
    card.append(h("div", { class: "hint", text: `Declared tools: ${p.tools.map((t) => `${t.name} (${t.access})`).join(", ")}. Only the read-only one above is ever called.` }));
  }

  for (const s of p.secrets) {
    const field = h("input", { type: "password", autocomplete: "off", spellcheck: "false", placeholder: s.present ? "••••••••  (stored)" : s.label, style: "flex:1 1 auto;min-width:0" }) as HTMLInputElement;
    const dotEl = statusDot(s.present);
    const saveBtn = h("button", { text: "Save" });
    saveBtn.addEventListener("click", async () => {
      try {
        await Bridge.secretSet(`plugin:${p.id}:${s.key}`, field.value.trim());
        dotEl.style.background = field.value.trim() ? "#22c55e" : "#f4505e";
        field.placeholder = field.value.trim() ? "••••••••  (stored)" : s.label;
        field.value = "";
      } catch (err) {
        feedback.replaceChildren(h("div", { class: "notice err", text: String(err) }));
      }
    });
    card.append(h("div", { class: "row" }, h("label", { text: s.label }), field, saveBtn, dotEl));
  }

  const actions = h("div", { class: "row" });
  if (p.approval === "on") {
    const off = h("button", { class: "danger", text: "Switch off" });
    off.addEventListener("click", async () => {
      await Bridge.pluginDisable(p.id).catch((e) => feedback.replaceChildren(h("div", { class: "notice err", text: String(e) })));
      rerender();
    });
    actions.append(off);
    if (p.kind === "mcp-stdio" || p.kind === "mcp-http") {
      const tools = h("button", { text: "Show the server's tools" });
      tools.addEventListener("click", async () => {
        try {
          const list = await Bridge.pluginTools(p.id);
          feedback.replaceChildren(h("div", { class: "hint", text: list.map((t) => `${t.name}${t.readOnlyHint ? " (read-only, says the server)" : ""}`).join(", ") || "No tools." }));
        } catch (err) {
          feedback.replaceChildren(h("div", { class: "notice err", text: String(err).replace(/^Error:\s*/, "") }));
        }
      });
      actions.append(tools);
    }
  } else {
    const trust = h("input", { type: "checkbox" }) as HTMLInputElement;
    const on = h("button", { class: "primary", text: "Switch on" }) as HTMLButtonElement;
    on.disabled = true;
    trust.addEventListener("change", () => (on.disabled = !trust.checked));
    on.addEventListener("click", async () => {
      try {
        // Pinned to the manifest shown here: if it changed meanwhile, this is refused.
        await Bridge.pluginEnable(p.id, p.hash);
        rerender();
      } catch (err) {
        feedback.replaceChildren(h("div", { class: "notice err", text: String(err).replace(/^Error:\s*/, "") }));
      }
    });
    actions.append(h("label", { style: "display:flex;gap:6px;align-items:center" }, trust, h("span", { text: "I reviewed this and trust it" })), on);
  }
  card.append(actions, feedback);
  return card;
}

function pluginsSection(): HTMLElement {
  const list = h("div", { style: "display:flex;flex-direction:column;gap:12px" });
  async function render() {
    const plugins = (await Bridge.pluginsList()) ?? [];
    clear(list);
    if (plugins.length === 0) {
      list.append(h("div", { class: "hint", text: "No plugins yet. A plugin is a folder holding a plugin.json; put it in the plugins folder." }));
    }
    for (const p of plugins) list.append(pluginCard(p, () => void render()));
  }
  void render();
  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Plugins" })),
    h("div", { class: "hint", text: "Status sources you add yourself: a web address read on a timer, or an MCP server. Nothing runs until you review it and switch it on, and a plugin that changes is switched off until you look again." }),
    h("div", { class: "row" },
      h("button", { text: "Open the plugins folder", onclick: () => void Bridge.pluginsOpenFolder() }),
      h("button", { text: "Look again", onclick: () => void render() }),
    ),
    list,
  );
}

// ── Tray and notifications section ────────────────────────────────────────────

function shellSection(): HTMLElement {
  const island = h("select", {}) as HTMLSelectElement;
  island.append(
    h("option", { value: "off", text: "Tray only" }),
    h("option", { value: "on", text: "Tray and the island at the top" }),
  );
  island.value = settings.island;
  island.addEventListener("change", () => {
    settings.island = island.value as Settings["island"];
    void save();
  });

  const notify = h("select", {}) as HTMLSelectElement;
  notify.append(
    h("option", { value: "needsYou", text: "When a session needs me or fails" }),
    h("option", { value: "all", text: "Also when a session finishes" }),
    h("option", { value: "off", text: "Never" }),
  );
  notify.value = settings.notifications;
  notify.addEventListener("change", () => {
    settings.notifications = notify.value as Settings["notifications"];
    void save();
  });

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Tray and notifications" })),
    h("div", {
      class: "hint",
      text: "Click the tray icon for your sessions, permission requests and services. With the island off, a permission request is answered in the terminal unless the tray panel is open, so Claude Code never waits on a card nobody sees.",
    }),
    h("div", { class: "row" }, h("label", { text: "Show Coucou in" }), island),
    h("div", { class: "row" }, h("label", { text: "Notifications" }), notify),
  );
}

// ── General section ───────────────────────────────────────────────────────────

function generalSection(): HTMLElement {
  const volume = h("input", {
    type: "range", min: "0", max: "0.2", step: "0.005",
    value: String(settings.soundVolume),
  }) as HTMLInputElement;
  volume.addEventListener("input", () => {
    settings.soundVolume = Number(volume.value);
    void save();
  });

  const autoClose = h("input", {
    type: "number", min: "5", max: "120", step: "1",
    value: String(Math.round(settings.autoCloseInterval)),
    style: "width:72px",
  }) as HTMLInputElement;
  autoClose.addEventListener("change", () => {
    settings.autoCloseInterval = Math.max(5, Math.min(120, Number(autoClose.value) || 15));
    autoClose.value = String(settings.autoCloseInterval);
    void save();
  });

  const screen = h("select", {}) as HTMLSelectElement;
  screen.append(
    h("option", { value: "primary", text: "Main display" }),
    h("option", { value: "cursor", text: "Display under the cursor" }),
  );
  screen.value = settings.screen;
  screen.addEventListener("change", () => {
    settings.screen = screen.value as Settings["screen"];
    void save();
  });

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "General" })),
    h("div", { class: "row" },
      h("label", { text: "Sound" }),
      toggle(settings.soundEnabled, (v) => { settings.soundEnabled = v; void save(); }),
      volume,
    ),
    h("div", { class: "row" },
      h("label", { text: "Auto-close" }),
      autoClose,
      h("span", { class: "hint", text: "seconds after you leave the island" }),
    ),
    h("div", { class: "row" },
      h("label", { text: "Island lives on" }),
      screen,
    ),
    h("div", { class: "row" },
      h("label", { text: "Launch at startup" }),
      toggle(settings.autostart, (v) => { settings.autostart = v; void save(); }),
    ),
  );
}

// ── Boot ──────────────────────────────────────────────────────────────────────

async function main() {
  const boot = await Bridge.boot();
  if (boot) {
    settings = { ...settings, ...boot.settings };
    version = boot.version;
  }
  const status = (await Bridge.hooksStatus()) ?? {
    installed: false, settingsPath: "", hookPath: "", hookReady: false,
  };

  const hasKey = (await Bridge.secretPresent("anthropic-api-key")) ?? false;
  const serverKey = (await Bridge.secretPresent("llm-server-api-key")) ?? false;
  const launch = await Bridge.launchInfo();

  const keys = [
    "stripe-api-key", "github-token", "vercel-token",
    "n8n-url", "n8n-api-key", "resend-api-key", "notion-api-key", "calcom-api-key",
  ];
  const present: Record<string, boolean> = {};
  for (const k of keys) present[k] = (await Bridge.secretPresent(k)) ?? false;

  clear(root);
  root.append(
    h("h1", {}, h("span", { text: "Coucou" }), h("span", { class: "version", text: version })),
    claudeSection(status),
    shellSection(),
    openInSection(launch),
    chatSourceSection(serverKey),
    apiSection(hasKey),
    integrationsSection(present),
    pluginsSection(),
    generalSection(),
    h("div", {
      class: "hint",
      text: "No telemetry. Network requests only go to the services you configure yourself.",
    }),
  );

  void onEvent<Settings>("settings-changed", (s) => {
    settings = { ...settings, ...s };
  });
}

void main();
