# Claude Code and Codex

Coucou uses the official CLIs and their existing account sign-ins. It has no separate login and never reads or copies their credentials. Both agents have the same connection, chat and project-launch controls.

## Connect

1. Install [Claude Code](https://code.claude.com/docs/en/setup) or [Codex](https://developers.openai.com/codex/cli) using its official instructions.
2. Open **Settings → Agents**. Coucou checks both installed CLIs. An existing Claude subscription or ChatGPT sign-in appears as connected.
3. If needed, choose **Sign in**, finish the official CLI's browser flow, then choose **Refresh connections**.
4. Choose **Use for all text** beside either connected agent to use it for chat, dictation cleanup, call summaries and voice. **Use for chat** changes chat alone. Open **Chat** in the tray, or press **Ctrl+Alt+Space**.

The combined action changes provider choices only. Dictation cleanup and voice still need their switches enabled in Flow Settings; microphone, playback, hotkey and performance preferences are retained. Each feature can also select a different provider in Flow Settings.

API credentials are optional separate chat providers. A CLI signed in only with an API key is not shown as a subscription connection. Plan limits and model availability belong to the selected agent's account.

### Claude API models

In **Settings → Chat → Claude**, save an API key and choose **Model**. The list loads from the configured Anthropic account on opening Settings and after changing the key. **Refresh** checks it again. The initial choices include Sonnet 5.5, Opus 5.5, Haiku 5.5 and Fable 5.1 from [Anthropic's model catalog](https://platform.claude.com/docs/en/models/overview). Account discovery supplies the available choices; a refresh retains the saved model even if it is absent from the list.

The selected model applies to Claude API chat and to Flow text tasks that select **Claude (your key)**, including call summaries. It does not change either official CLI's model or switch a subscription to API billing. Model-discovery errors are shown beside the picker; a failed settings save restores the saved choice and can be retried.

## Chat and project work

Chat answers questions and accepts UTF-8 text attachments up to 200 KB, PNG/JPEG/GIF/WebP images up to 5 MiB, and PDFs up to 20 MiB and twenty pages on Windows. It retains conversation context and offers a Stop control. The displayed provider identifies which account is answering. The native CLI chooses its default model; Coucou does not pin a model ID. Chat has no shell, project-editing, MCP or web-search tools.

Both CLI providers receive the same image content. Windows renders every PDF page locally, in order, at a maximum long edge of 1,600 pixels. A document exceeding the page, image or rendering limits is rejected as a whole. Dense text can be less legible after rendering; use a text attachment for exact text analysis. Limits live in `src-tauri/src/cli_agents/attachment-limits.json`.

Claude Code receives image blocks over stdin. Codex receives private temporary image files through its official image option; those files are removed when the request completes, fails or is stopped. Abrupt process termination can leave temporary files in Coucou's private data directory. Attachments remain available for follow-up questions with the same provider. Switching providers retains the plain conversation without transferring attachment bytes; start a new chat to attach the file to the other provider.

For coding, enter an existing absolute **Project folder** and choose **Open project** beside either agent. The full official agent opens in its own terminal, in that folder. Its own project instructions, skills, MCP configuration, model picker and session controls remain available. Claude Code uses manual permission mode; Codex uses on-request approval and a workspace-write sandbox. Coucou never selects bypass mode or approves a tool on its own.

Hook monitoring is separate from signing in. In **Settings → Agents**, review the hook diff and choose Install for each agent you want to follow. The installer backs up each changed file and refuses stale previews. Codex project launches enable its hooks feature; trust the hooks once through Codex's **/hooks** interface. For sessions started elsewhere, enable the hooks feature in Codex as well. Claude Code loads its configured hooks normally.

Both agents appear in the tray and the island. Windows preferences control the island and notifications. A permission is actionable only while a suitable app surface is visible. The tray has Allow/Deny controls but no multiple-choice form: questions return to the native terminal when the island is off. Closing Coucou leaves decisions with the agent.

## Configuration

The running app inherits these optional environment settings:

| Setting | Meaning |
|---|---|
| `COUCOU_CLAUDE_BIN` | Absolute path of the Claude Code executable |
| `COUCOU_CODEX_BIN` | Absolute path of the Codex executable |
| `CLAUDE_CONFIG_DIR` | Claude Code's own configuration directory; hook setup uses the same directory |
| `CODEX_HOME` | Codex's own account/configuration directory; hook setup uses the same directory |

Without executable overrides, Coucou checks PATH and standard per-user install locations. Restart Coucou after changing its environment. No executable is downloaded by the connection controls; Install opens the official instructions.

The connection view returns only installation and subscription-sign-in status. Chat prompts travel over stdin, not process arguments. Output and conversation sizes are bounded. CLI checks time out after 15 seconds; a chat reply after 180 seconds. Stop and timeout end the supervised process tree on Windows. Inherited API key variables are removed from subscription commands.

CLI chat runs in Coucou's private chat directory with project rules and tools disabled. Full project sessions run with native interactive configuration. User hooks are changed only through the separate preview-and-install flow.

## Verification and boundaries

Run from `windows/`:

```powershell
npm ci
npm test
npm run build
cargo test --workspace
```

The explicit live checks use installed, signed-in accounts and consume a small amount of their plan usage:

```powershell
cargo test --lib cli_agents::tests::live_ -- --ignored --nocapture
```

Automated coverage includes subscription detection, completed versus partial/error output, timeouts, attachment history and provider boundaries, native PDF rendering, temporary-file cleanup, no-key provider selection, cancellation controls, independent agent sessions and approval handback. Live visual checks ask each CLI to identify a synthetic image and read distinct project names on both pages of a synthetic PDF. [Integration review](integration-review.md) records observed verification and remaining gates.

Project launch from Settings is implemented on Windows. Linux returns an instruction to open the agent in a terminal; native Linux and macOS behavior are not validated by the Windows checks. Phone-link CLI chat requires a separate opt-in. The proactive assistant in [agent-design.md](agent-design.md) is a design document, not an implemented feature.
