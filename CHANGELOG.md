# Changelog

## Unreleased

- Windows: plugins — status sources you add as a folder with a plugin.json: a web address read on a timer, or an MCP server (stdio or HTTP, protocol 2026-07-28 with a fallback for 2025-11-25) whose read-only tool is polled. They show in the tray flyout. A plugin runs only after you review and approve exactly its manifest; hosts are allowlisted and pinned, secrets stay in the Credential Manager, and MCP programs start without a shell, in a job object. The built-in integrations run on the same mechanism; settings.json moves from activeIntegrations to plugins, keeping a dated backup.
- Windows: the chat can answer with your own model server — llama.cpp's llama-server, LM Studio, Ollama or anything that speaks the OpenAI chat API — instead of Claude. Plain http only on this computer and the local network, no redirects, the key in the Credential Manager; text files work, PDFs and images still need Claude.
- Windows: Coucou lives in the notification area. The tray icon shows what is going on and who needs you; a click opens a Windows 11 style flyout with every session, the permission card and your services, and jumps to a session's window. Questions and failures raise Windows notifications with a button that goes to the session. The island at the top of the screen is now optional and off by default.
- Windows: every Claude Code session is followed on its own. The pill counts them, puts the one that needs you first and says where it runs (Windows Terminal, Cursor, VS Code) and whether it is in bypass mode; one session finishing no longer marks the others finished.
- Windows: a question from Claude Code opens a question card instead of Allow / Deny, and **Go to Windows Terminal** / **Go to Cursor** brings that session's window to the front — the exact Windows Terminal window, or the editor window of its workspace. ↗ on the Claude Code pill does the same.
- Windows: starting Coucou leaves the keyboard where it was; the hidden settings window and tray flyout no longer take it at launch.
- Windows: a new version of Coucou replaces the hook relay even while Claude Code hooks are running it; the old copy is moved aside and removed on a later start.
- Windows: Settings → Open projects in picks what the island's buttons open — Cursor, VS Code or nothing; Windows Terminal, a PowerShell window or nothing — with File Explorer as the fallback.
- Declare the tools you use in Settings: Gemini CLI, Antigravity, Anthropic, Google AI and OpenAI pills join the existing ones (Cursor and Codex pills are coming soon), and you pick the main pill.
- Chat now supports Google AI (Gemini) and OpenAI in addition to Anthropic; switch provider and model by clicking the model name in the chat view, on macOS.
- Linux version: the Tauri app now builds for Linux too (AppImage, .deb, .rpm), with the island as a layer-shell overlay on Wayland and Claude Code hooks over a private Unix socket (#21) — thanks @Davy133
- Compact island on screens without a notch (#22) — thanks @Kamasoutra
- Only web links (http/https) open from the notch; other kinds of links from Claude or integrations are ignored (#16) — thanks @Cris1670
- Hook socket limited to your own user account, with size and time limits; logs no longer keep commands, n8n data or full URLs, and stay under 1 MB (#16) — thanks @Cris1670 and @Vignesh-Thangamariappan
- The island always reopens after folding, and Settings opens below it, resizable — thanks @rouderz
- Choose the Claude model for the chat in Settings; the list comes from your Anthropic account, and Claude Sonnet 4.6 stays the default — thanks @rouderz
- Windows build artifacts are now downloadable from a manual CI run — thanks @MysJofR
- Any agent can talk to Mochi: tag a hook payload with `coucou_agent` (e.g. `nb-hook --agent my-agent`) and it gets its own pill in the island (#7, #9) — thanks @lacatu5
- Gemini CLI and Antigravity (agy) hook support on macOS: install from Settings and their sessions show up in the island — thanks @corefusiion
