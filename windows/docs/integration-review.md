# Windows integration review

Verification snapshot: 2026-10-28. Scope: remote integration, existing-account connections for Claude Code and Codex, Windows session/tray behavior, chat and settings preservation.

## Compared inputs

| Input | Commit |
|---|---|
| Local feature branch | `e3165455d3765a28859d8b82bddee7eda528003c` |
| Fetched `origin/main` | `47d1c2da12bf1dce8fb28b825757d497c1e1d88d` |
| Fetched `fork/main` | `4d57215c6db08f2513299143c59234069453bb22` |
| Fetched tracked feature branch | `02f918cbeb31e0ad79be8beb94e151061b15f16b` |

The comparison contains 704 upstream-only commits and 15 local-only commits. Upstream changes span 1,037 files, including 217 under `windows/`. The integration includes upstream's multi-agent hooks and app features together with the local Windows tray, session details, notifications, focus, project-launch and folder-plugin behavior. The 29 merge conflicts have resolved content; no conflict entries or markers remain.

## Reviewed behavior

- **Both agents:** existing subscription sign-in detection, an explicit native sign-in action, no-key chat selection, cancellation and full native project launch. Claude Code 2.1.311 and Codex CLI 0.165.0 are the installed versions used for live checks.
- **Chat completion:** Claude event arrays and single-result JSON require a successful terminal result. Codex requires a completed turn. Partial, empty and failed responses do not become successful conversation turns. Text attachments remain in follow-up context.
- **Process lifetime:** bounded stdin/output, deadlines and cancellation; Windows job supervision ends child processes. Concurrent CLI chat requests are rejected until the active turn finishes or stops. Full project sessions use native manual/on-request approval.
- **Permissions and sessions:** Claude Code and Codex both populate tray rows and retain independent activity. Hidden surfaces hand requests back. A question never becomes an Allow/Deny card in the tray. Ticker state and diff IDs are preserved when aggregate badges change. Native agent processes are recognized for terminal targeting and liveness.
- **Configuration:** existing Windows preferences and folder-plugin hashes are retained. Legacy built-in selections and model-server preferences are imported with a backup before replacement. The custom-server credential is address-bound. Ordinary settings saves cannot grant plugin trust, and storage errors are returned to the caller.
- **Hook setup:** preview, fingerprint verification, backup and explicit installation remain required. Claude and Codex configuration-directory overrides are honored. An IDE port is carried as `ide_port`, not a process ID.
- **Tests and localization guard:** provider/schema assertions include the new connections and retained preferences. Existing behavior checks remain. The localization scan distinguishes CSS and view identifiers from visible text using syntax; a regression fixture checks both accepted identifiers and rejected untranslated labels.

## Observed verification

| Check | Result |
|---|---|
| `npm test` | 589 passed, zero failed |
| `cargo test --workspace` | 720 passed: 671 app, 46 hook, 3 relay; six ignored |
| Explicit live CLI smoke tests | Both Claude Code and Codex passed sign-in and real reply checks |
| TypeScript and Vite production frontend build | Passed |
| Tauri Windows debug build, no bundle, isolated identifier | Passed |
| Isolated Windows process startup | Running and responding; startup log observed |
| Merge conflict entries and markers | None |
| Whitespace check against `origin/main` | Passed for the integration delta |

Two ignored Rust tests are the live CLI checks run explicitly above. The other four require Android/relay interoperability or speech/wake-word model fixtures; they were not run. Tests do not install hooks into the user's real agent configuration.

The full staged comparison against the local input reports upstream Markdown whitespace in `docs/ANDROID_LINK.md` and `docs/INTEGRATIONS.md`. The Windows build also reports an existing unused `VK_LMENU` constant and a Vite chunk-size warning. These checks were not disabled.

## Open verification gates

- Native visual inspection and clicking through Settings, login browser flows, project launch and hook approval round trips for each CLI. Both Windows UI automation and browser inspection hosts are unavailable in this environment; process startup alone does not establish these workflows.
- Linux/macOS builds and native behavior. Settings project launch currently returns a terminal instruction on Linux; Linux toast delivery is not implemented.
- New connection controls use English copy. Full translation coverage of those controls is not established.
- Installer packaging/signing and release delivery. This review does not validate every feature in the 704 upstream commits or establish whole-app production readiness.

For the supported user flow, see [coding-agents.md](coding-agents.md). The proactive assistant in [agent-design.md](agent-design.md) is specification only.
