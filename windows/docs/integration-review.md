# Windows integration review

Verification snapshot: 2026-10-30. This review covers the integrated Windows build, existing Claude Code/Codex sign-ins, chat, session monitoring, audio capture and native feature equivalents. See [Windows feature coverage](windows-parity.md) for implementation details and per-feature limits.

## Remote comparison

The integrated branch contains upstream `7b228f1e6529f16623dd859d9e7f33966816f19c`; a fresh fetch reports zero missing upstream commits. The local Windows tray, session details, notifications, focus, project launch and folder-plugin behavior are retained. Claude Code and Codex remain explicit, equally supported connections. There is no implicit API-to-CLI provider fallback and no unresolved merge entry.

## Reviewed behavior

- Claude Code and Codex have equal connection, sign-in, chat, image/PDF and native project controls. Live checks use Claude Code 2.1.312 and Codex CLI 0.166.0. Subscription credentials remain with the official CLIs.
- Both connections offer **Use for all text**: an explicit click selects chat, dictation cleanup, call summaries and voice. Flow provider changes retain audio, hotkey, performance and enabled switches. Failed saves remain visible.
- CLI completion requires a successful terminal result. Processes have bounded input/output, cancellation, deadlines and Windows child-process supervision. Native projects retain manual/on-request approvals.
- Attachment history stays with its selected provider. PDF conversion includes every page within explicit limits; switching providers does not transfer attachment bytes. Screenshot pasting, window selection and drag attachment preview context without sending.
- Session permissions remain independent for both agents. Hidden surfaces hand requests back; tray questions are not converted into Allow/Deny approvals. Hook installation requires preview, fingerprint verification, backup and an explicit click.
- Settings migration retains Windows preferences, model-server settings and folder-plugin trust. Ordinary settings saves cannot grant trust. Credentials use the OS store.
- Automatic call audio follows active application routes before Windows defaults, with independent fixed-device overrides, shared Settings/recording recovery and retained errors. Ambiguous app or browser routes are reported.
- Finished calls retain source names, received-audio evidence, peak levels, native formats and warnings after transcription removes the audio files. Discontinuity warnings do not claim that a finished call is still recording.
- Floating controls stay mounted during tooltip changes, tooltips open inward, and click-through regions use rendered geometry. Dictation status awaits native setup before reporting listener availability.
- Service and portfolio integrations remain read-only. Configured refresh loops are bounded and inactive while disabled or paused. Missing data is visibly incomplete.

## Observed verification

| Check | Result |
| --- | --- |
| Frontend suite | 638 passed, zero failed |
| Rust workspace suite | 781 passed: 732 app, 46 hook, 3 relay; 21 opt-in tests ignored |
| TypeScript and Vite production frontend | Passed; the large shared translation-chunk warning remains unsuppressed |
| Rust compile | Passed without warnings |
| CI for source commit `7fab876ccbb3a34eb3e48b2cd0e1a1b0d72e6571` | [Windows checks](https://github.com/lazarevtill/coucou/actions/runs/40973331802) and [macOS/iPhone checks and unsigned builds](https://github.com/lazarevtill/coucou/actions/runs/40973331909) passed |
| Windows installer | NSIS 0.5.5 built and installed successfully; complete executable matches the compiled release with Tauri's NSIS bundle marker |
| Installed startup | Process remains responsive and the native log confirms 0.5.5, the selected Right Alt listener and Parakeet loading in about 2.3 seconds; the follow-up GUI behavior is not yet verified |
| Installation preservation | Eighteen checked files unchanged immediately after installation, including notes, models, preferences and both CLI configurations. Startup changes only the updater's last-check metadata. Bundled and staged hook executables match |
| Installed speech models | All six files match the manifest sizes and SHA-256 hashes; an optimized native check passed transcription, automatic calibration and manual override |
| Both official CLI subscriptions | Real text replies and structured text tasks passed |
| Both CLI visual paths | Real image identification and both pages of a synthetic PDF passed |
| Windows PDF and attachment handling | Native rendering, cancellation, limits, history separation and temporary-file cleanup passed |
| Native Windows Share | Cross-thread file preparation and attachment readback passed |
| Snipping Tool | Installed Microsoft package discovery passed |
| Audio | Laptop mic plus Bluetooth playback passed; the current build's laptop mic plus built-in speakers check passed at 48 kHz stereo. Both streams reported recoverable discontinuities and continued delivering audio |
| App audio routing | Separate native capture/playback fixture detected on its actual endpoints |
| Parakeet | Synthetic transcription, CPU calibration, manual override and speaker separation passed; the installed-model release check measured four threads fastest at about 247 ms per six-second sample |
| Markets | Public quotes and nine public wallet-service checks passed |
| Apple Music | Installed app discovery and 5% per-app volume verified; latest probe has no active media session |

The opt-in tests require signed-in providers, native applications, model fixtures, hardware interaction or Android/relay interoperability. Selected native checks are run explicitly as described above; the ignored count is not a claim that all hardware paths passed. Tests do not install hooks into the user's real CLI configuration.

## Open verification gates

- Native desktop control is stopped by its Escape interruption state. Follow-up toolbar, startup-status, capture-details and provider-selection GUI checks remain open, along with real project/hook approval round trips, screenshot capture/paste and mail-client drafts.
- Real Zoom/Meet device changes, in-flight headset disconnection/reconnection, wired/USB hardware and actual call transcription need interactive verification. The latest endpoint probe exposes only the built-in microphone and speakers.
- Apple Music needs a playable song and active media session for control, seek and artwork verification. Private exchange accounts and other configured service accounts are not tested.
- The Android phone protocol has a Windows implementation; Windows-to-iPhone transport is not implemented. The [NetBird note](netbird.md) describes integration options, not an implemented remote-management panel.
- The installed build is a local validation build. No public release or upstream PR is published by these checks.
- Linux builds and native macOS/iPhone behavior remain outside this validation. Full app production readiness is not established by the passing suites and unsigned CI builds.

The fourteen-language catalogs pass completeness and placeholder checks; native linguistic and layout review remains open. The proactive assistant in [agent-design.md](agent-design.md) is a specification.
