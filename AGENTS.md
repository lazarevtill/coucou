# Coding instructions

Read [CLAUDE.md](CLAUDE.md) for the repository structure and product rules. macOS code is in `NotchBuddy/`; Windows/Linux code is in `windows/`.

- Keep Claude Code and Codex equally supported for connection status, sign-in, chat, project launch and session monitoring. Use the official CLIs; never read their credential files or implement a separate OAuth flow.
- Keep both CLI sign-ins available for text tasks and opted-in phone chat. Speech performance uses measured local runtime capabilities, with a persisted manual override. Call capture must show source health and preserve gaps and warnings after device recovery.
- Market integrations remain read-only, run only for enabled pills, and show unavailable prices as unknown. Keep credentials in the OS credential store.
- Native coding sessions retain user-controlled tool permissions. Chat subprocesses are supervised, bounded and cancellable, with tools disabled.
- Hook installation requires a reviewed diff, a successful backup and an explicit user click. Never auto-approve a tool or plugin.
- Preserve Windows preferences, folder-plugin trust and native session focus when integrating upstream changes. Keep provider/schema contracts and behavior assertions together.
- Run the frontend and Rust workspace checks listed in `windows/README.md`. Explicit live CLI smoke tests require installed signed-in accounts; report actual results and platform limits.
- Do not commit, push, merge a PR/MR, publish or deploy without explicit authorization in the current session. No force push without separate permission. No co-author trailers or generated footers.
- Make small, cohesive commits with clear plain messages. After commit and push are authorized, push all task commits to the intended branch and verify the remote SHA.
- Fix causes, do not suppress checks. Keep secrets out of code, commands, logs and documentation. Keep docs current, with detailed instructions under `docs/`.
- Long runs keep local state in ignored `_prive/` files. Do not add personal infrastructure details to project documentation.
