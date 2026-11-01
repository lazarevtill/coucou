# Coding instructions

Coucou shows AI coding agent sessions through Mochi, an animated character. macOS code is in `NotchBuddy/`; Windows/Linux code is in `windows/`. The Android app is in `android/`, the iPhone Live Activity (APNs) relay in `relay/` and the Android–desktop link relay in `android/relay/`.
Read [CLAUDE.md](CLAUDE.md) for the Mac structure and product rules.

## Find what you need
1. Run `git ls-files '*.md'` to list every doc; paths and file names say what each covers (`tree docs` prints only folders on Windows).
2. Open the docs for the area you change and read their headings first (`grep -n '^#' <file>`), then the sections you need. Area rules are under `## Windows and Linux rules` below.
3. Each doc names the code paths it covers; go to the code from there.
4. Before changing any doc, read [docs/document-writing.md](docs/document-writing.md).

## Checks
- Windows and Linux: `cd windows && npm ci && npm run tauri dev`
- Windows checks: `npm test`, `npm run build`, `cargo test --workspace` from `windows/`. Details are in `windows/README.md`.
- Run `python -X utf8 scripts/test-whats-new.py` from the repository root after an upstream sync. Windows and Linux What's new entries in `windows/src/whats-new/whats-new.json` use the 16 languages, not the 14 in CLAUDE.md.
- Explicit live CLI smoke tests require installed signed-in accounts; report actual results and platform limits.

## Rules everywhere
- Keep Claude Code and Codex equally supported for connection status, sign-in, chat, project launch, tray behavior and session monitoring. Use the official CLIs; never read their credential files or implement a separate OAuth flow.
- Hook installation requires a reviewed diff, a successful backup and an explicit user click. Never auto-approve a tool or plugin.
- Preserve Windows preferences, folder-plugin trust and native session focus when integrating upstream changes. Keep provider/schema contracts and behavior assertions together.
- Do not commit, push, merge a PR/MR, publish or deploy without explicit authorization in the current session. No force push without separate permission. No co-author trailers or generated footers.
- Keep commits small, cohesive and readable, with clear plain messages. Once commit and push are authorized, push all task commits to the intended branch and verify the remote SHA. Keep upstream merges separate from subsequent feature and fix commits.
- Fix causes, do not suppress checks. Keep secrets out of code, commands, logs and documentation. Keep docs current, with detailed instructions under `docs/`.
- Security audits include resolved Android build plugins and native speech libraries, not only package lockfiles. Android's `dependencySecuritySbom` init task exports the actual app, test and build-plugin graph. Do not hide findings or infer native binary safety from a clean lockfile scan. Dependency security checks and unresolved native/Linux findings are in `windows/docs/dependency-security.md`.
- Long runs keep local state in ignored `_prive/` files. Do not add personal infrastructure details to project documentation.

## Windows and Linux rules

### Coding agents, chat and project launch
- Claude Code and Codex connections are in `windows/src-tauri/src/cli_agents.rs` and `windows/src/settings/connections.ts`. See `windows/docs/coding-agents.md` and `windows/docs/integration-review.md`.
- Keep both CLI sign-ins available for text tasks and opted-in phone chat.
- Native coding sessions retain user-controlled tool permissions. Chat subprocesses are supervised, bounded and cancellable. Claude Code and Codex chat use the shared runner with tools disabled, including subscription aliases.
- `cli_relay.rs` adds opt-in subscription switches for the upstream providers. Anthropic and OpenAI subscription aliases must use the same `cli_agents` runtime, attachment handling and cancellation as explicit Claude Code/Codex. Keep API model choices and billing separate. Other relays use official CLI read-only modes (their plan/ask modes) with bounded process supervision; their inherited configuration is not an OS sandbox. Do not claim those modes isolate inherited CLI configuration or grant tool/URL permissions automatically. A failed settings save must leave the subscription switch unchanged.
- Phone chat keeps its explicit enable switch and model allowlist and validates them independently. Selected subscription routes do not require API credentials; the picker offers Claude Code/Codex aliases only after their sign-in probes succeed. Unavailable CLI subscriptions must not fall back to an API credential. Subscription-only providers expose the CLI's default model.
- The Claude API model picker uses `claude_api_models` with the stored API key for account discovery even when chat uses a subscription. Settings opening, key changes and Refresh update the list without changing the saved model; never silently migrate it to a newer model. Keep known choices and show discovery failures; failed model saves restore the saved selection. Claude API text tasks use this same model preference.
- `windows/src-tauri/src/local_chat.rs` owns model-server chat. Settings migration must retain Windows preferences and plugin trust, preserve explicit current values, and back up a migrated settings file before replacing it. Generic settings saves cannot grant plugin trust.
- Model clients must not follow HTTP redirects to unconfigured endpoints. Keep loopback proxy bypass and bounded responses.
- Windows project launches resolve known editor batch entry points to native executables before passing folder arguments; unknown batch scripts must never receive a project path.

### Local MCP
- Local MCP access to saved calls is read-only and uses `windows/src-tauri/src/mcp/` and `windows/src/settings/mcp.ts`. `coucou mcp` starts the official SDK's stdio server before Tauri initialization. Its separate permission is off by default, rechecked per request and before returning data; ordinary preferences cannot grant it. Never initialize the GUI, credential stores or migrations in MCP mode; never register clients or grant access automatically. Search/fetch are the only tools. Preserve opened-handle path checks, input/read bounds, revision-checked pagination, cancellation and explicit incomplete-note diagnostics. Setup covers Claude Code, Claude Desktop, Codex CLI and desktop clients without silently changing their files or tool permissions. See `windows/docs/local-mcp.md`.

### Notes and call audio
- Speech and call recording use `windows/src-tauri/src/speech/` and `notes/`. Flow Settings selects text providers, audio devices and automatic or manual CPU threads. Speech performance uses measured local runtime capabilities, with a persisted manual override. Source recovery runs independently of transcription; lost-audio warnings remain in the note. Call capture must show source health and preserve gaps and warnings after device recovery. See `windows/docs/windows-parity.md` for feature coverage and hardware verification limits.
- Parakeet calibration uses at most six seconds from the most energetic window of a successful transcription. Leading silence must not invalidate tuning. Every candidate must produce a stable, matching transcript before its timing can win; the source transcript is unchanged and no calibration audio is saved. Retain the manual thread override.
- Notetaker saves final source diagnostics in each note: used device names, audio receipt, peak level and warnings survive transcription and audio deletion. Received packets do not prove speech, and a WASAPI discontinuity does not prove a device disconnect.
- `speech/source.rs` shares audio health and reconnection between Notetaker and the no-file Settings test. `speech/routing.rs` follows the active call application's Windows audio sessions before system defaults. Manual microphone and playback choices remain independent and fixed; ambiguous browser/app routes must be visible instead of guessed. Hardware diagnostics are ignored tests that require explicit live-test authorization.
- Audio-route process grouping uses title-free window ownership: discovery reads process ownership only, never window titles or UI messages. Never call `GetWindowTextW` on Coucou's windows from a recording worker: it synchronously waits for the UI thread. Capture start/stop and writer draining run off the UI thread, including floating-toolbar actions; never hold recording locks across calls into that thread.
- Notetaker retains one Record/Stop button across timer and note updates. Pending actions disable it; failed start/stop commands leave the error visible and permit retry. The stop bridge must propagate failures.
- Final transcription retains existing text on screen during processing and failure, and emits `notes-progress` by source. Progress counts examined audio; speaker analysis has a stage and elapsed time, without a fabricated percentage. Only exact digital silence skips model inference. A failed model chunk fails the operation instead of saving a partial result as complete. Note writes are serialized; `notes/storage.rs` saves and syncs the final transcript before removing source audio. Failed commits retain recordings and the prior transcript for retry.
- Windows uses the Mac's short-speaker-fragment rules with temporal matching because Sherpa does not expose per-turn embeddings. Giving two remote speakers the same name combines their lines; microphone identity stays separate. Automatic speaker labels are estimates, not verified identities.
- Windows note exports use `notes/export.rs` and the native Save As dialog. Its dedicated COM apartment keeps the main UI responsive and preserves Windows overwrite confirmation. Cancellation writes no file; the page prevents duplicate dialogs and reports write failures and the chosen saved path. Export must not alter the stored note.

### Flow dictation
- `flow_status` awaits native setup before returning listener availability.
- The connections view's Use for all text action selects chat and uses `flow_use_agent` to patch the three Flow text providers under the store lock. It must retain audio, hotkey, performance and enabled switches, and report a failed Flow save without claiming complete setup.
- The Flow toolbar uses `windows/src/pill/surface.ts` to retain its capsule and controls through tooltip updates. Tooltips sit outside the capsule layout, toward the screen interior, without changing the capsule's position. Keep native input regions and frontend hover geometry aligned through animations and DPI changes.
- Dictionary, snippet and app-style commands commit their JSON file before updating the Flow store. Their pages use `hub/save.ts` to prevent overlapping edits and show native write errors. Keep entered text and the saved list on failure; only clear a submitted form after success. A failed style selection returns to its saved value.
- History deletion and erasure also await disk writes and propagate errors through the Flow bridge. Keep an entry visible until deletion succeeds, retain the erase confirmation on failure and prevent overlapping submissions. Erasing everything writes history first, then statistics; if the second write fails, the error explicitly states that history was erased and statistics remain. Live values follow each successful write so retry and reload agree.
- Windows dictation may consume only the explicitly selected Copilot chord while enabled. Ignore injected keys, preserve ordinary modifier shortcuts, and keep physical-key verification distinct from event-sequence tests.

### Island, Hub and tray
- User requests from shortcuts and tray commands use `Island.openExplicit`, so chat, wardrobe, recap and update cards remain available with the idle island off. `Island.alert` is for background events and respects that setting.
- `hub_shown` takes its initial visibility snapshot on the native UI queue. WebView2 can process page commands during setup before queued event listeners are installed; reading the startup atomic immediately can leave visible canvases stopped. Keep the initial snapshot ordered after native setup and retain the hidden-window animation gate, so canvases draw on first open and stop while hidden or minimized.
- Hub pages use the available window width. Notetaker uses flexible call-list and detail columns, with a container query to stack them in narrow windows. The compact history scrolls independently with Record/Stop pinned at its top. Audio details use two columns when they fit; text and action rows wrap. CSS resizing must preserve mounted controls and input drafts.

### Attachments and file sharing
- Chat window attachments use short-lived, single-use selections in `chat_input.rs`. `platform/window_context.rs` reads only the chosen browser's navigation chrome, excludes document/password controls, and returns only an unambiguous web address. Preview context before Send; do not add background browser collection.
- The Windows Attach front window shortcut records its target before the island opens. Its separate single-use offer expires after ten seconds. Keep Windows/Linux shortcut availability and native/frontend contract assertions aligned.
- On Windows, dragging Mochi from the island onto an app uses the same single-use context offer. `context_window_at_point` respects window stacking and refuses to select behind an unreadable top window. Moving Mochi after placing him on the desktop only changes his position.
- `cli_agents/attachments.rs` shares image and PDF handling between Claude Code and Codex. Windows renders every PDF page locally within bounded page counts, size and time; reject oversized documents without sending partial pages. Keep native attachment history separate from provider-neutral text history. Codex temporary files are private and scoped to the supervised request; Claude images travel over stdin. Provider changes must not transfer attachment bytes automatically.
- `chat_images.rs` and `views/chat-images.ts` provide explicit screenshot pasting with bounded PNG storage and preview; screenshot pixels enter chat only through that paste. The capture button launches the verified Microsoft Snipping Tool package. Capture and annotation stay in that native app; Coucou never polls the clipboard, silently captures a screen or sends an attachment without Send.
- `windows/src-tauri/src/file_share.rs` opens Windows Share for a copied inbox file. Its native attachment is prepared off the UI thread and marshalled through an agile reference. Only explicit clicks open the chooser; the user selects the mail application and sends the draft.

### Markets and music
- Public crypto quotes and read-only portfolios use `windows/src-tauri/src/markets/`, with request limits and service origins in its `defaults.json`. They must remain read-only and inactive until the user enables the pill. Portfolio keys stay in the OS credential store; unavailable balances and prices must remain visibly incomplete. Keep Android catalog metadata aligned with shared pill IDs.
- Music state includes the native session's `canSeek` capability (Windows media controls or MPRIS CanSeek). A disabled seek bar remains read-only and never shows an optimistic position; native Windows commands also recheck that capability before changing position. Apple Music and Spotify retain separate state and commands.

### Phone link relay (`android/relay/`)
- Phone relays preserve binary frame order, including bursts and hibernation. Share the access-key generation check instead of racing per-frame digests; retain key-rotation, frame-size and rate-limit checks.
