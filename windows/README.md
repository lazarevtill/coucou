# Coucou for Windows

Coucou follows coding-agent sessions from the tray and an optional island at the top of the screen. It shows activity, diffs and permission requests, opens the session's terminal, and provides chat.

## Claude Code and Codex

1. Open **Settings → Agents**. Existing Claude Code and Codex/ChatGPT sign-ins are detected automatically.
2. Choose **Use for all text** beside either agent for chat, dictation cleanup, call summaries and voice. Open **Chat** from the tray or press **Ctrl+Alt+Space**. The optional voice and cleanup switches are in Flow Settings.
3. To code with the full agent, enter a **Project folder** and choose **Open project**. Its tools, skills, MCP setup and permission prompts stay in its native terminal.
4. To monitor sessions, review and install each agent's hooks in Settings. Codex also requires trusting the hooks with **/hooks**.

No Coucou account is required. API keys and local model servers remain optional. See [agent setup and limits](docs/coding-agents.md) and [chat sources](docs/model-sources.md).

## Use

The tray opens chat, settings and session activity. **Settings → General** controls the island, notifications, editor and terminal. The island can show other supported agents and service integrations, plan usage, weekly recaps and diffs. The phone link, dictation, notes and voice features have their own controls in the Coucou window.

- [Terminal detection and focus](docs/terminals.md)
- [Folder plugins](docs/plugins.md)
- [Current integration verification](docs/integration-review.md)
- [Windows features, call audio and Parakeet settings](docs/windows-parity.md)
- [NetBird remote-management integration note](docs/netbird.md)
- [Project documentation](../docs/)

## Build and verify

Windows development requires Node.js, Rust with the MSVC toolchain, Microsoft C++ Build Tools and WebView2.

```powershell
npm ci
npm test
npm run build
cargo test --workspace
npm run tauri dev
```

`npm run build` also builds the hook relay. `npm run tauri build` creates the Windows bundles. See [coding-agents.md](docs/coding-agents.md) for optional tests against signed-in CLIs. Build and test output is not a release or deployment.

The published [Windows releases](https://github.com/Louis-CFM/coucou/releases/tag/windows-latest) are upstream builds; unpublished work in this branch requires a local build. Linux uses the same frontend and much of the Rust backend, with platform-specific code. This Windows integration does not establish Linux or macOS release readiness.
