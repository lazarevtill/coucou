# Windows feature coverage

The Windows app uses native Windows services where the Mac uses Apple APIs. The original Mac implementation is the comparison source; a feature being present in source does not establish hardware or account verification.

| Workflow | Windows implementation | Verification still needed |
| --- | --- | --- |
| Claude Code and Codex sign-in, chat, projects | Official installed CLIs and existing sign-ins; equal connection controls | Native click-through and hook approval round trip |
| Agent activity, diffs, permissions, questions, terminal focus | Hooks, tray, island and native Windows focus | End-to-end session approval on installed build |
| Cloud and local model chat | API providers, Ollama, LM Studio, configured model servers | Real accounts and local servers beyond CLI checks |
| Flow dictation, dictionary, snippets, styles and statistics | Parakeet, Windows keyboard handling, local history and selectable cleanup provider | Native microphone and paste in third-party apps |
| Parakeet performance | Measured CPU thread selection and manual override; bundled runtime is CPU-only | GPU/NPU runtime support is not implemented |
| Notetaker Record a call | Separate microphone and WASAPI playback sources; device selection, live meters, timestamp alignment, speaker separation, summaries and export | Real-call and Bluetooth/wired/USB device matrix |
| Voice assistant | Local recognition and system speech; Claude API, Claude Code, Codex or model-server brain; explicit mail drafts | Microphone, playback and spoken action round trip |
| Service pills | Stripe, n8n, GitHub, GitLab, Gitea, Vercel, Resend, Notion, Cal.com and WhatsApp | Configured service accounts |
| Spotify | Windows system media controls | Installed Spotify playback |
| Wardrobe, desktop Mochi, sounds, shortcuts, displays, recap | Existing Windows frontend and native integration | Native visual and interaction checks |
| Phone approval and chat | Android phone link with device authentication | Live Android pairing and remote flow |
| Apple Music | Dedicated native integration, commands, state, artwork, per-app volume, pill and voice actions | Playback controls and artwork on a playing track; discovery and 5% volume verified on the installed app |
| Crypto prices | Coinbase and OKX public quotes, 24-hour charts, currency selection and opt-in movement alerts | Native settings and pill interaction |
| Portfolio | Read-only exchange balances, 16 native wallet networks, USD valuation and partial-data indicators | Real exchange accounts and each public wallet service |
| Chat microphone and window context | Local dictation fills the editable prompt; user-selected Windows app name and window title accompany a new chat | Native microphone and window picker click-through; browser URL capture is not implemented |
| Traditional Chinese and Thai | Complete Windows catalogs and fourteen-language picker; native locale and plural rules | Native linguistic and layout review |
| Apple iPhone/iCloud features | Windows has Android pairing; no iPhone transport equivalent yet | Cross-platform phone protocol and iPhone implementation |

## Call audio

Choose the actual microphone and the output used by the call application under Flow Settings. Automatic choices use the Windows communications endpoints, falling back to system defaults. Manual choices retain their device identity. A short audio test reports levels without writing audio to disk.

The playback source captures the digital mix on the selected endpoint, including other applications on that endpoint. It works with speakers, wired headphones, USB audio and supported Bluetooth endpoints. Recording cannot bypass a device's own bandwidth limits: enabling a Bluetooth headset microphone can change its playback profile. A separate laptop or USB microphone can avoid that limitation. The app shows the active source name and native format, and reports missing or failed sources.

Microphone and playback are stored separately at the 16 kHz mono rate required by the speech models. A shared timeline preserves silence and device reconnection gaps. Disk writes run outside the audio callback. Transcript echo filtering removes repeated speaker playback from the microphone transcript; it is not an acoustic noise-cancellation driver. Audio is deleted after transcription succeeds. Summary generation sends text only to the explicitly selected provider.

## Performance evidence

A synthetic speech fixture on the development laptop completed transcription and automatic calibration, then produced the same transcript with the manual one-thread setting. The six-second calibration sample took about 613 ms at one thread, 413 ms at two, 370 ms at four, 500 ms at eight and 1,250 ms at sixteen. These measurements describe that machine and fixture, not a universal performance guarantee. Automatic mode measures the user's machine and stores timings, never calibration audio.

## Public crypto prices

Enable Crypto under Settings → Integrations, choose Coinbase or OKX, and save up to eight symbols. Requests run once per minute only while the pill is enabled and Coucou is not paused; history is cached for ten minutes. No account is required. OKX dollar pairs are labelled USDT. Missing prices remain unknown, and each failed source shows its error. Alerts are off by default and measure movement since the last alert or first successful quote. Service endpoints and request limits are in `src-tauri/src/markets/defaults.json`; preferences are stored separately in `markets.json`.

## Portfolio

Enable Portfolio under Settings → Integrations. Binance, OKX, Bybit, Kraken and Coinbase Advanced Trade use read-only keys, tested before saving to the OS credential store. Coinbase uses an ECDSA private key. The settings page never reads a saved key back. Disconnect removes that exchange's credentials.

Public wallet addresses support Bitcoin, Dogecoin, Litecoin, Ethereum, Polygon, BNB Chain, Avalanche, Arbitrum, Optimism, Base, Solana, TRON, XRP, Cardano, Cosmos and NEAR. These show native coin balances only; token, staking and derivative positions are not included. Public services receive the address being checked. Addresses and labels live in `portfolio.json`; account balances remain in memory.

Refresh runs only while the pill is enabled and Coucou is not paused. Each account reports its own failures. Missing prices stay unknown, and a partial total is labelled accordingly. USD prices and seven-day charts use public Coinbase endpoints. Service origins and request limits are in `src-tauri/src/markets/portfolio/defaults.json`.

## Apple Music

Apple Music and Spotify have separate pills, commands, event streams and player state. Each talks directly to its own native application and keeps sign-in in that application. Shared code handles only Windows media APIs and the common player layout.

Apple Music volume targets its own Core Audio sessions, including its packaged library agent, across active speaker and headphone outputs. Other players and the system volume are not changed. Playback actions use the selected song or queue in Apple Music. If the application has no playable selection, open Apple Music and choose a song.
