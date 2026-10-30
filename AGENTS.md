# Coding instructions

Read [CLAUDE.md](CLAUDE.md) for the repository structure and product rules. macOS code is in `NotchBuddy/`; Windows/Linux code is in `windows/`.

- Keep Claude Code and Codex equally supported for connection status, sign-in, chat, project launch and session monitoring. Use the official CLIs; never read their credential files or implement a separate OAuth flow.
- Keep both CLI sign-ins available for text tasks and opted-in phone chat. Speech performance uses measured local runtime capabilities, with a persisted manual override. Call capture must show source health and preserve gaps and warnings after device recovery.
- Phone chat keeps its explicit enable switch and model allowlist. A selected subscription must not require an API key; unavailable CLI subscriptions must not fall back to an API credential.
- Speech calibration selects a bounded segment with audio from a successful transcription, including recordings with leading silence. Keep the original transcript unchanged and retain the manual thread override.
- Automatic call audio follows observed application routes before Windows defaults; manual microphone and playback choices stay independent and fixed. Use the same recovery path for Settings meters and recording. Report ambiguous browser/app routes instead of guessing.
- Audio-route discovery reads process ownership only, never window titles or UI messages. Open and stop capture off the UI thread; never hold recording locks across calls into that thread.
- Keep floating toolbar buttons mounted during tooltip updates. Tooltips open inward without changing the capsule's position; native click-through and hover must use the rendered geometry.
- Explicit shortcut and tray requests open their views even when the idle island is off. Keep automatic alerts subject to that preference.
- Read the Hub's initial visibility on the native UI queue after startup. Its canvases must draw on first open and stop while hidden or minimized.
- Keep Notetaker's Record/Stop control mounted through clock ticks and note updates. Prevent duplicate pending actions and show start/stop failures so the user can retry.
- Windows note export uses the native Save As dialog on a dedicated COM apartment. Cancellation writes nothing; retain native overwrite confirmation and show export failures without losing the note.
- Dictionary, snippet and app-style edits must reach disk before replacing live values. Keep drafts and saved lists on failure, show the error and prevent overlapping submissions.
- Wait for Flow setup before reporting keyboard-listener availability. Saved call diagnostics retain devices, received audio, peak levels and warnings; never infer speech or a disconnect from a discontinuity alone.
- Windows dictation may consume only the explicitly selected Copilot chord while enabled. Ignore injected keys, preserve ordinary modifier shortcuts, and keep physical-key verification distinct from event-sequence tests.
- Market integrations remain read-only, run only for enabled pills, and show unavailable prices as unknown. Keep credentials in the OS credential store.
- Music seeking must follow the selected player's advertised native capability. Do not send unsupported seeks or show an optimistic position for them; keep Apple Music and Spotify state independent.
- Native coding sessions retain user-controlled tool permissions. Chat subprocesses are supervised, bounded and cancellable. Claude Code and Codex chat use the shared runner with tools disabled, including subscription aliases. Other upstream subscription relays are explicit opt-ins using their official plan/ask modes; do not claim those modes isolate inherited CLI configuration or grant tool/URL permissions automatically.
- Keep visual attachments equivalent for both CLI providers. Render every PDF page within explicit limits; reject oversized documents without sending partial pages. Provider changes must not transfer attachment bytes automatically.
- Screenshot pixels enter chat only through an explicit paste. Preview them before Send; never monitor the clipboard or silently capture a screen.
- Hook installation requires a reviewed diff, a successful backup and an explicit user click. Never auto-approve a tool or plugin.
- Preserve Windows preferences, folder-plugin trust and native session focus when integrating upstream changes. Keep provider/schema contracts and behavior assertions together.
- Run the frontend and Rust workspace checks listed in `windows/README.md`. Explicit live CLI smoke tests require installed signed-in accounts; report actual results and platform limits.
- Do not commit, push, merge a PR/MR, publish or deploy without explicit authorization in the current session. No force push without separate permission. No co-author trailers or generated footers.
- Make small, cohesive commits with clear plain messages. After commit and push are authorized, push all task commits to the intended branch and verify the remote SHA.
- Fix causes, do not suppress checks. Keep secrets out of code, commands, logs and documentation. Keep docs current, with detailed instructions under `docs/`.
- Long runs keep local state in ignored `_prive/` files. Do not add personal infrastructure details to project documentation.
