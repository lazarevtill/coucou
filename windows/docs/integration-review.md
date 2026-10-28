# Windows integration review

Verification snapshot: 2026-10-29. This review covers the integrated Windows build, existing Claude Code/Codex sign-ins, chat, session monitoring, audio capture and native feature equivalents. See [Windows feature coverage](windows-parity.md) for implementation details and per-feature limits.

## Remote comparison

The integrated branch contains upstream `47d1c2da12bf1dce8fb28b825757d497c1e1d88d`; a fresh fetch reports zero missing upstream commits. Its comparison inputs are local `e3165455d3765a28859d8b82bddee7eda528003c` and fork `4d57215c6db08f2513299143c59234069453bb22`. The integrated content includes 704 upstream-only commits and preserves the local Windows tray, session details, notifications, focus, project-launch and folder-plugin behavior. There are no unresolved merge entries.

## Reviewed behavior

- Claude Code and Codex have equal connection, sign-in, chat, image/PDF and native project controls. Live checks use Claude Code 2.1.311 and Codex CLI 0.165.0. Subscription credentials remain with the official CLIs.
- CLI completion requires a successful terminal result. Processes have bounded input/output, cancellation, deadlines and Windows child-process supervision. Native projects retain manual/on-request approvals.
- Attachment history stays with its selected provider. PDF conversion includes every page within explicit limits; switching providers does not transfer attachment bytes. Screenshot pasting, window selection and drag attachment preview context without sending.
- Session permissions remain independent for both agents. Hidden surfaces hand requests back; tray questions are not converted into Allow/Deny approvals. Hook installation requires preview, fingerprint verification, backup and an explicit click.
- Settings migration retains Windows preferences, model-server settings and folder-plugin trust. Ordinary settings saves cannot grant trust. Credentials use the OS store.
- Automatic call audio follows active application routes before Windows defaults, with independent fixed-device overrides, shared Settings/recording recovery and retained errors. Ambiguous app or browser routes are reported.
- Service and portfolio integrations remain read-only. Configured refresh loops are bounded and inactive while disabled or paused. Missing data is visibly incomplete.

## Observed verification

| Check | Result |
| --- | --- |
| Frontend suite | 621 passed, zero failed |
| Rust workspace suite | 768 passed: 719 app, 46 hook, 3 relay; 21 opt-in tests ignored |
| TypeScript and Vite production frontend | Passed; existing large shared-chunk warning remains |
| Rust compile | Passed without warnings |
| Both official CLI subscriptions | Real text replies and structured text tasks passed |
| Both CLI visual paths | Real image identification and both pages of a synthetic PDF passed |
| Windows PDF and attachment handling | Native rendering, cancellation, limits, history separation and temporary-file cleanup passed |
| Native Windows Share | Cross-thread file preparation and attachment readback passed |
| Snipping Tool | Installed Microsoft package discovery passed |
| Audio | Laptop mic plus Bluetooth playback passed; laptop mic plus speakers after disconnect passed |
| App audio routing | Separate native capture/playback fixture detected on its actual endpoints |
| Parakeet | Synthetic transcription, CPU calibration, manual override and speaker separation passed |
| Markets | Public quotes and nine public wallet-service checks passed |
| Apple Music | Installed app discovery and 5% per-app volume verified; latest probe has no active media session |

The opt-in tests require signed-in providers, native applications, model fixtures, hardware interaction or Android/relay interoperability. Selected native checks are run explicitly as described above; the ignored count is not a claim that all hardware paths passed. Tests do not install hooks into the user's real CLI configuration.

## Open verification gates

- The native computer-control pipe and browser inspection host are unavailable. Visual inspection, real login/project/hook approval round trips, screenshot capture/paste and mail-client drafts remain unverified.
- Real Zoom/Meet device changes, in-flight headset disconnection/reconnection, wired/USB hardware and actual call transcription need interactive verification. The latest endpoint probe exposes only the built-in microphone and speakers.
- Apple Music needs a playable song and active media session for control, seek and artwork verification. Private exchange accounts and other configured service accounts are not tested.
- Installer packaging and installation are not yet verified. No public release or upstream PR is published by these checks.
- Linux/macOS builds and native behavior remain outside this Windows validation. Full app production readiness is not established by the passing suites.

The fourteen-language catalogs pass completeness and placeholder checks; native linguistic and layout review remains open. The proactive assistant in [agent-design.md](agent-design.md) is a specification.
