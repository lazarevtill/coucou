# Coucou — guide for AI coding agents

Coucou is a native macOS app (`NotchBuddy/`); `windows/` is the Tauri version for Windows and Linux. Mochi, a small animated character living in the MacBook notch, shows AI coding agent sessions (Claude Code, Gemini CLI, Antigravity and more) and a few integrations, and lets the user approve, answer, chat and drop files from the notch.

## Where things are
- `NotchBuddy/Sources/App/` — Mac-only Swift code. `NotchBuddy/Sources/CoucouKit/` — code shared with the iPhone app (Mochi's BotEngine and outfits, pills, diff, models). `NotchBuddy/Sources/Phone/` — iPhone app (`CoucouPhone` target), with its widgets in `Sources/Widgets/` and the expanded approval notification in `Sources/NotificationContent/`. `NotchBuddy/Resources/sounds/` — the 28 WAV sounds. `NotchBuddy/project.yml` — XcodeGen project (never edit the `.xcodeproj` by hand).
- `NotchBuddy/Sources/CoucouKit/PillCatalog.swift` — single source of truth for all declared pills (workspace tools, agents, AI providers, services). Every pill ID, color, category and subtitle lives here.
- `docs/SPEC.md`, `docs/INTEGRATIONS.md` — behaviour, views, states, integrations (in French).
- `design/prototype/notch-buddy.html` — original prototype, the visual source of truth. `design/captures/` — target screenshots.
- `windows/` — the Tauri app for Windows and Linux: Rust in `src-tauri/`, TypeScript in `src/`, the `coucou-hook` relay in `hook/`. `windows/README.md` lists what differs from the Mac.
- `docs/*.html` — the GitHub Pages site (privacy, terms, support, legal notice).
- `relay/` — the Coucou relay, a stateless Cloudflare Worker that forwards the iPhone Live Activity pushes (it holds the APNs key, which must never ship in an app). See `relay/README.md`.

## Build
```
cd NotchBuddy && xcodegen && xcodebuild -scheme NotchBuddy -configuration Debug build
```
To run it day to day on the Mac (signed, iPhone link, permissions kept across builds): `./scripts/dev-install.sh` — it installs the only Coucou in /Applications and removes the copies earlier builds left.
Windows and Linux: `cd windows && npm ci && npm run tauri dev`

Windows checks: `npm test`, `npm run build`, `cargo test --workspace` from `windows/`.
Claude Code and Codex connections are in `windows/src-tauri/src/cli_agents.rs` and `windows/src/settings/connections.ts`. Both use official CLI sign-ins; credentials stay with the CLI. Keep both agents covered when changing connections, chat, project launch or tray behavior. See `windows/docs/coding-agents.md` and `windows/docs/integration-review.md`.

The connections view's Use for all text action selects chat and uses `flow_use_agent` to patch the three Flow text providers under the store lock. It must retain audio, hotkey, performance and enabled switches, and report a failed Flow save without claiming complete setup.

Public crypto quotes and read-only portfolios use `windows/src-tauri/src/markets/`, with request limits and service origins in its `defaults.json`. They must remain read-only and inactive until the user enables the pill. Portfolio keys stay in the OS credential store; unavailable balances and prices must remain visibly incomplete.

Speech and call recording use `windows/src-tauri/src/speech/` and `notes/`. Flow Settings selects text providers, audio devices and automatic or manual CPU threads. Source recovery runs independently of transcription; lost-audio warnings remain in the note. See `windows/docs/windows-parity.md` for feature coverage and hardware verification limits.

The Flow toolbar uses `windows/src/pill/surface.ts` to retain its capsule and controls. Tooltips sit outside the capsule layout, toward the screen interior. Keep native input regions and frontend hover geometry aligned through animations and DPI changes.

`flow_status` awaits native setup before returning listener availability. Notetaker saves final source diagnostics in each note: used device names, audio receipt, peak level and warnings survive transcription and audio deletion. Received packets do not prove speech, and a WASAPI discontinuity does not prove a device disconnect.

`speech/source.rs` shares audio health and reconnection between Notetaker and the no-file Settings test. `speech/routing.rs` follows the active call application's Windows audio sessions before system defaults. Manual microphone and playback choices remain independent; ambiguous app routes must be visible. Hardware diagnostics are ignored tests that require explicit live-test authorization.

Audio-route process grouping uses title-free window ownership. Never call `GetWindowTextW` on Coucou's windows from a recording worker: it synchronously waits for the UI thread. Capture start/stop and writer draining run off the UI thread, including floating-toolbar actions.

`windows/src-tauri/src/local_chat.rs` owns model-server chat. Settings migration must retain Windows preferences and plugin trust, preserve explicit current values, and back up a migrated settings file before replacing it. Generic settings saves cannot grant plugin trust.

`windows/src-tauri/src/file_share.rs` opens Windows Share for a copied inbox file. Its native attachment is prepared off the UI thread and marshalled through an agile reference. Only explicit clicks open the chooser; the user selects the mail application and sends the draft.

Chat window attachments use short-lived, single-use selections in `chat_input.rs`. `platform/window_context.rs` reads only the chosen browser's navigation chrome, excludes document/password controls, and returns only an unambiguous web address. Preview context before Send; do not add background browser collection.

The Windows Attach front window shortcut records its target before the island opens. Its separate single-use offer expires after ten seconds. Keep Windows/Linux shortcut availability and native/frontend contract assertions aligned.

On Windows, dragging Mochi from the island onto an app uses the same single-use context offer. `context_window_at_point` respects window stacking and refuses to select behind an unreadable top window. Moving Mochi after placing him on the desktop only changes his position.

`cli_agents/attachments.rs` shares image and PDF handling between Claude Code and Codex. Windows renders PDFs locally with bounded page counts, size and time. Keep native attachment history separate from provider-neutral text history. Codex temporary files are private and scoped to the supervised request; Claude images travel over stdin.

`chat_images.rs` and `views/chat-images.ts` provide explicit screenshot pasting with bounded PNG storage and preview. The capture button launches the verified Microsoft Snipping Tool package. Capture and annotation stay in that native app; Coucou never polls the clipboard or sends an attachment without Send.

## Rules
- Keep commits small, cohesive and readable, with clear plain messages. Once commit and push are authorized, push all task commits to the intended branch and verify the remote SHA. Keep upstream merges separate from subsequent feature and fix commits.
- Swift 6, SwiftUI + AppKit. No third-party dependencies unless truly unavoidable. The character is drawn in code (`Canvas` + `TimelineView`), no Rive/Lottie/images.
- Secrets live in the Keychain, never on disk or in git.
- No telemetry. Network calls only to services the user configured.
- Never block Claude Code: if the app doesn't answer, the hook exits immediately.
- Never overwrite `~/.claude/settings.json`: dated backup, merge, show the diff, write only after the user confirms.
- Never send an email or approve a Claude Code or Codex permission without an explicit click.
- Performance: 0 % CPU when the island is hidden.
- Keep the bundle identifier `fr.louisraille.NotchBuddy` (Keychain items, preferences and permissions depend on it).
- Never restyle what already ships (pills, cards, Settings, chat…): existing views stay exactly as they are in `main`, which is the App Store build. Change the look of an existing view only when explicitly asked.
- Pill IDs are stable contract values (Keychain, UserDefaults, hook routing): never rename an existing pill ID.
- New views follow the existing app style. `design/prototype/notch-buddy.html` and `design/captures/` are references for new work, not a reason to change existing views.
- Every release adds its CHANGELOG.md section, a row in the README Versions table, and commits the regenerated Info.plist with the new version.
- Every release also adds its What's new, shown once in the notch or the island after Coucou updates itself: an entry for the new version in `NotchBuddy/Resources/whats-new.json` (Mac, the 14 languages) or `windows/src/whats-new/whats-new.json` (Windows and Linux, the 14 languages). 1 to 4 items picked from the CHANGELOG section, each with an icon from `scripts/test-whats-new.py`, a short title and one line, translated in every language. `python3 scripts/test-whats-new.py` checks it; CI and `release.sh` fail without it.
