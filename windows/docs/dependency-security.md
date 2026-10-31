# Dependency security

Audit date: 2026-10-31. Scope: the desktop dependency graph, Android app and build tools, both Cloudflare relays, declared native speech components and GitHub Actions advisories. This is not an operating-system audit or a claim that the application has no vulnerabilities.

## Current results

| Scope | Result |
| --- | --- |
| Frontend and both relay npm lockfiles | `npm audit` reports zero vulnerabilities in each project |
| Android resolved dependencies | OSV reports zero advisories across 218 distinct Maven coordinates, including app runtime, JVM tests and build-plugin classpaths |
| Rust workspace lockfile | No CVE-tagged results; the Linux GLib vulnerability and four maintenance advisories below remain visible |
| Native source inventory | OSV commit queries for 43 declared source revisions report no matches. The separately published Sherpa CVE below demonstrates why this does not establish native binary safety |
| Mac speech dependency | FluidAudio 0.17.7 has no external Swift package dependencies. Its bundled NemoTextProcessing 0.3.1 source lockfile has 68 Rust packages and no OSV findings; the Apple binary has not been rebuilt or inspected on Windows |
| CI actions | Published action advisories were checked. The download-artifact traversal advisory affects versions below 4.1.3; this repository uses v8. Floating action/runtime tags and runner-installed software are not a complete reproducible binary inventory |

OSV-Scanner 2.7.0 was checked against its official release asset SHA-256. Scans include all vulnerability results, with no ignored advisory IDs, severity cutoff or reachability suppression. Source manifests and upstream release metadata complement the registry scans; they do not attest to every component of a downloaded binary.

## Pinned security updates

- Frontend: Vite **8.3.5**, using its native Rolldown and Oxc build options. Node.js **22.18+** is required.
- Android relay: Vitest **5.0.5**, Cloudflare Vitest plugin **1.4.11**, Wrangler **4.156.0** and Workers types **5.20261031.1**. Peer dependency checks are enabled.
- APNs relay: Wrangler **4.156.0** and Workers types **5.20261031.1**.
- Rust networking: rustls **0.23.46**, rustls-webpki **0.103.16**, webpki-roots **1.0.11** and Tokio **1.54.1**.
- Local MCP: official Rust SDK `rmcp` **3.8.0**, with server and stdio features only. The 659-package Rust rescan reports no additional advisories from this dependency.
- Speech: sherpa-onnx **2.0.1**, zip **9.2.0**, zstd **0.14.3**, zstd-safe **8.2.0** and zstd-sys **2.1.2+zstd.1.5.7**. The calibration cache identifies the runtime version. The downloaded Windows native archive matches its official release digest; the release's CMake files select ONNX Runtime **1.28.2** for Windows/Linux x64.
- Android build: AGP **9.5.0**, Kotlin Compose plugin **2.4.21**, Gradle **9.8.2** with a distribution checksum and verified wrapper. Built-in Kotlin uses the Java 17 target. `localeFilters` retains the exact supported language list.
- Android build-only constraints: Bouncy Castle **1.86**, Guava **33.7.2-jre**, Commons Lang **3.21.0**, JDOM **2.0.6.1** and jose4j **0.9.7**. These correct vulnerable transitive selections still present in the current Android plugin; they are not additional APK dependencies.

## Open findings

**Sherpa: CVE-2026-108954.** The native decoder trusts vocabulary metadata when reading the joiner's output. [Upstream issue 3983](https://github.com/k2-fsa/sherpa-onnx/issues/3983) remains open. Inspection of stable 2.0.1 finds the unchecked bounds in both the generic decoder and the NeMo decoder used for Parakeet; upgrading the Rust wrapper does not fix this native code. Coucou downloads only pinned official models and verifies download hashes, but existing cached model files are checked by size. Malformed-model handling is therefore an unresolved dependency issue. A maintained native fix and regression validation are required before claiming it is closed.

**Linux GLib: RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g.** `glib` 0.18.5 has an unsound string iterator. The [published fix](https://github.com/advisories/GHSA-wrw7-89jp-8q8g) is in 0.20.0 and later. Tauri's GTK3 dependency and Coucou's Linux window integration require the 0.18 API; the current stable Tauri release still uses it. Replacing just the transitive version is not a compatible fix. The affected dependency is not compiled into Windows builds.

**Maintenance advisories.** `proc-macro-error` (RUSTSEC-2024-0370), `zbus_names` (RUSTSEC-2026-0371), `zvariant` (RUSTSEC-2026-0370) and `zvariant_derive` (RUSTSEC-2026-0372) remain in the cross-platform lockfile. These are maintenance notices, not CVEs. The standalone D-Bus types moved into zbus 6; the current Tauri single-instance plugin still selects zbus 5. Keep these findings visible until their owning dependencies support migration.

## Repeat the audit

Use an installed, verified OSV-Scanner executable. From the repository root:

```powershell
osv-scanner scan source --all-vulns --all-packages --lockfile windows/Cargo.lock --lockfile windows/package-lock.json --lockfile android/relay/package-lock.json --lockfile relay/package-lock.json
```

Run `npm ci` and `npm audit` separately in `windows/`, `android/relay/` and `relay/`. For Android, export the graph Gradle actually resolves, including build plugins:

```powershell
cd android
./gradlew.bat -I scripts/security-dependencies.gradle dependencySecuritySbom
osv-scanner scan source --all-vulns --all-packages --lockfile build/reports/dependency-security.cdx.json
```

The export fails if any dependency is unresolved. On Linux/macOS use `./gradlew`. Review native release manifests and advisories separately: Cargo/npm/Maven inventories do not fully describe statically linked libraries, Apple frameworks, WebView2, system WebKitGTK or runner software.

## Verification

The full Windows frontend run has 716 passing tests, including MCP desktop setup and settings search. All five production pages load in headless Edge without script or network failures. The Rust workspace has 852 passing tests; 22 opt-in tests are excluded from the default run. Explicit model tests pass for transcription, diarization, automatic calibration and manual thread selection using isolated synthetic audio and model storage. Both Claude Code and Codex pass live MCP search/fetch checks using synthetic calls.

Android has 1,010 unit tests: 1,008 pass and two opt-in end-to-end cases require their relay harness. Lint, debug APK, optimized unsigned release APK and the release log-removal check pass. Both relay projects pass their applicable typecheck and dry Worker build checks; the phone relay has 41 passing tests. These results do not establish macOS/Linux runtime readiness or verify a new installed Windows GUI build.
