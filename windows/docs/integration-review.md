# Windows integration review

Verification snapshot: 2026-10-30. This review covers the integrated Windows build, existing Claude Code/Codex sign-ins, chat, session monitoring, audio capture and native feature equivalents. See [Windows feature coverage](windows-parity.md) for implementation details and per-feature limits.

## Remote comparison

The integrated branch contains upstream 0.5.6 at `37e457ddae3cbe7be4a6539cc5b1cd968692971c`; a fresh fetch reports zero missing upstream commits. The Windows tray, session details, notifications, focus, project launch and folder-plugin behavior are retained. Claude Code and Codex remain explicit, equally supported connections. There is no implicit API-to-CLI provider fallback and no unresolved merge entry.

## Reviewed behavior

- Claude Code and Codex have equal connection, sign-in, chat, image/PDF and native project controls. Live checks use Claude Code 2.1.312 and Codex CLI 0.166.0. Subscription credentials remain with the official CLIs.
- Both connections offer **Use for all text**: an explicit click selects chat, dictation cleanup, call summaries and voice. Flow provider changes retain audio, hotkey, performance and enabled switches. Failed saves remain visible.
- Optional Anthropic/OpenAI subscription aliases use the same Claude Code/Codex runtime. Phone chat retains its separate enable switch and model allowlist without requiring an unrelated API key. Other upstream CLI relays are opt-in and use their official plan/ask modes; inherited configuration is not an OS sandbox.
- CLI completion requires a successful terminal result. Processes have bounded input/output, cancellation, deadlines and Windows child-process supervision. Native projects retain manual/on-request approvals.
- Attachment history stays with its selected provider. PDF conversion includes every page within explicit limits; switching providers does not transfer attachment bytes. Screenshot pasting, window selection and drag attachment preview context without sending.
- Session permissions remain independent for both agents. Hidden surfaces hand requests back; tray questions are not converted into Allow/Deny approvals. Hook installation requires preview, fingerprint verification, backup and an explicit click.
- Settings migration retains Windows preferences, model-server settings and folder-plugin trust. Ordinary settings saves cannot grant trust. Credentials use the OS store.
- Automatic call audio follows active application routes before Windows defaults, with independent fixed-device overrides, shared Settings/recording recovery and retained errors. Ambiguous app or browser routes are reported.
- Finished calls retain source names, received-audio evidence, peak levels, native formats and warnings after transcription removes the audio files. Discontinuity warnings do not claim that a finished call is still recording.
- Floating controls stay mounted during tooltip changes, tooltips open inward, and click-through regions use rendered geometry. Dictation status awaits native setup before reporting listener availability.
- Notetaker retains its Record/Stop button during clock and transcript updates. Capture workers do not read window titles or synchronously call the UI while holding recording locks. Pending actions cannot be submitted twice; errors stay visible and retryable.
- Explicit shortcuts and tray commands can open requested views while the idle island is off. Automatic alerts continue to respect that preference.
- Service and portfolio integrations remain read-only. Configured refresh loops are bounded and inactive while disabled or paused. Missing data is visibly incomplete.

## Observed verification

| Check | Result |
| --- | --- |
| Frontend suite | 683 passed, zero failed |
| Rust workspace suite | 813 passed: 764 app, 46 hook, 3 relay; 22 opt-in tests ignored |
| TypeScript and Vite production frontend | Passed; the large shared translation-chunk warning remains unsuppressed |
| Rust compile | Passed without warnings |
| Remote CI | Unverified for the latest source. GitHub rejects workflow dispatch with HTTP 422, “Actions has been disabled for this repository,” and supplies no GitHub Actions run for the latest commit |
| Windows installer | NSIS 0.5.6 from source `9216aa826ba8882c78c6c9c605ab200fc8c8c8a1` built and installed successfully; complete executable matches the compiled release with Tauri's NSIS bundle marker |
| Installed startup | Native accessibility state shows 0.5.6 and Ready with the selected Right Alt listener. The process remains responsive; Parakeet loaded in about 2.9 seconds. The desktop helper cannot activate the current Hub window, so the new progress view has no completed native click-through check |
| Installation preservation | Twenty-two checked files unchanged immediately after installation, including notes, models, preferences and both CLI configurations |
| Native provider controls | Both Use for all text buttons completed successfully; all four targets changed correctly and unrelated preferences were retained. Original choices were restored after testing |
| Native summaries | Both Claude Code and Codex generated the expected summary, key points and Friday task from a synthetic call through the installed GUI. Original provider preferences were restored |
| Native Markdown export | Windows Save As cancellation wrote nothing; a subsequent save produced the complete 551-byte synthetic note export at the chosen path. The stored note was unchanged |
| Native dictionary, snippets and styles | Injected write failures showed errors and retained the draft or saved selection. Each succeeded after removing the failure and retrying. Original data and preferences were restored |
| History deletion and erasure | Four frontend regressions and four native store tests passed, including Windows file locks, retry, duplicate submissions, incoming dictations and partial statistics-erasure failure. Checks use isolated synthetic data |
| Hub artwork | Native startup, minimize/restore and close/reopen checks showed the animated logo, page characters and backgrounds. The initial visibility snapshot runs on the UI queue; existing hidden-window animation checks pass |
| Native Notetaker | Start, live transcript, Stop and saved Capture details passed with quiet synthetic speech. Stop retained its UI identity across clock/transcript updates, the full expected sentence was saved and the app remained responsive |
| Final call processing | The installed build processed a synthetic fixture with 30 minutes of leading microphone silence and 280 seconds of playback in 56.9 seconds. All 21 expected speech repetitions survived; native logs covered microphone, playback, speaker analysis and saving. The saved final note loaded correctly and its audio was removed. The synthetic note was then removed; five existing user notes retained their exact hashes |
| Processing UI and save failures | Frontend regressions retain transcript nodes across progress updates and display text after failure. Native write and atomic-replacement failures preserve source audio and the prior transcript; a successful retry commits before cleanup |
| Speaker grouping | Three native regressions cover brief fragments, short-only calls, repeated brief speakers and rename-to-combine without changing text or microphone identity. Actual speaker identities are not verified by these tests |
| UI deadlock regression | An unpumped native window reproduces the synchronous window-title wait before the fix; the title-free audio-route scan passes |
| Explicit-view regression | Five tests reproduce suppressed shortcut/recap/update requests with the island off; all pass with explicit opening. The installed chat shortcut expands the native window from 240 by 6 to 720 by 320 and focuses it. Visual and typing confirmation remains open because the desktop helper does not expose the floating window |
| Installed speech models | All six files match the manifest sizes and SHA-256 hashes; an optimized native check passed transcription, automatic calibration and manual override |
| Both official CLI subscriptions | Real text replies and structured text tasks passed |
| Claude API models | The configured account listed Sonnet 5.5, Opus 5.5, Haiku 5.5 and Fable 5.1. A short synthetic Sonnet 5.5 request returned the expected answer. Five picker regressions cover API/CLI separation, saved choices, late responses, failures and retry; native picker interaction remains unverified |
| Both CLI visual paths | Real image identification and both pages of a synthetic PDF passed |
| Windows PDF and attachment handling | Native rendering, cancellation, limits, history separation and temporary-file cleanup passed |
| Native Windows Share | Cross-thread file preparation and attachment readback passed |
| Snipping Tool | Installed Microsoft package discovery passed |
| Audio | Laptop mic plus Bluetooth playback passed; the current build's laptop mic plus built-in speakers check passed at 48 kHz stereo. Both streams reported recoverable discontinuities and continued delivering audio |
| App audio routing | Separate native capture/playback fixture detected on its actual endpoints |
| Native Settings audio test | Both built-in source names, meters and 48 kHz stereo formats appeared; the ten-second check stopped automatically and created no note. The microphone's interruption warning stayed visible |
| Parakeet | Synthetic transcription, leading-silence calibration, manual override and speaker separation passed. The native fixture measured four threads fastest at about 234 ms per six-second sample; the installed call measured two threads fastest for its sample |
| Markets | Public quotes and nine public wallet-service checks passed |
| Apple Music | Selected-track discovery, artwork, 5% per-app volume and both play/pause state changes passed the native control check. The current player session advertises seeking as unavailable; Coucou rejects it and keeps the progress bar read-only |

The opt-in tests require signed-in providers, native applications, model fixtures, hardware interaction or Android/relay interoperability. Selected native checks are run explicitly as described above; the ignored count is not a claim that all hardware paths passed. Tests do not install hooks into the user's real CLI configuration.

## Open verification gates

- Native chat for both providers, floating-toolbar interaction, real project/hook approval round trips, screenshot capture/paste and mail-client drafts need further GUI verification. Generic upstream CLI accounts are not installed on the development laptop.
- A real long Zoom call reached a saved transcript with device recovery evidence. Exact physical switch causes, lossless capture and speaker identities remain unverified. Controlled in-flight headset reconnection, Meet-specific changes and wired/USB hardware still need interactive verification. The latest native dual-capture test passed with built-in microphone and speakers at 48 kHz stereo; recoverable discontinuities remained visible.
- The updated installed progress view and speaker-merge controls need native interaction verification. The desktop helper reports “coordinate input geometry is unavailable” and “failed to activate captured window” after fresh window selection and retry. Frontend behavior tests and the installed processing pipeline pass independently of that helper.
- Apple Music's supported-seek path and sustained audible playback remain unverified. Native play/pause acceptance and artwork do not establish audio continuity. Private exchange accounts and other configured service accounts are not tested.
- The Android phone protocol has a Windows implementation; Windows-to-iPhone transport is not implemented. The [NetBird note](netbird.md) describes integration options, not an implemented remote-management panel.
- The installed build is a local validation build. No public release or upstream PR is published by these checks.
- Linux builds and native macOS/iPhone behavior remain outside this validation. Full app production readiness is not established by the passing suites and unsigned CI builds.

The sixteen-language catalogs pass completeness and placeholder checks; both platforms' 0.5.6 What's new entries pass. The macOS localization check passes with UTF-8 Python input. Native linguistic and layout review remains open. The proactive assistant in [agent-design.md](agent-design.md) is a specification.
