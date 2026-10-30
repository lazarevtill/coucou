# Windows feature coverage

The Windows app uses native Windows services where the Mac uses Apple APIs. The original Mac implementation is the comparison source; a feature being present in source does not establish hardware or account verification.

| Workflow | Windows implementation | Verification still needed |
| --- | --- | --- |
| Claude Code and Codex sign-in, chat, projects | Official installed CLIs and existing sign-ins; both native Use for all text controls verified | Native chat and hook approval round trip |
| CLI image and PDF chat | Same visual content for both agents; Windows renders complete PDFs locally with limits | Native file drop and chat click-through; signed-in image/PDF checks use synthetic fixtures |
| Agent activity, diffs, permissions, questions, terminal focus | Hooks, tray, island and native Windows focus | End-to-end session approval on installed build |
| Cloud and local model chat | API providers, Ollama, LM Studio, configured model servers | Real accounts and local servers beyond CLI checks |
| Flow dictation, dictionary, snippets, styles and statistics | Parakeet, Windows keyboard handling, local history and selectable cleanup provider | Native microphone and paste in third-party apps |
| Parakeet performance | Measured CPU thread selection and manual override; bundled runtime is CPU-only | GPU/NPU runtime support is not implemented |
| Notetaker Record a call | Separate microphone and WASAPI playback sources; device selection, live meters, timestamp alignment, speaker separation, summaries and export; native synthetic start/transcribe/stop verified | Real-call and Bluetooth/wired/USB device matrix |
| Voice assistant | Local recognition and system speech; Claude API, Claude Code, Codex or model-server brain; explicit mail drafts | Microphone, playback and spoken action round trip |
| Service pills | Stripe, n8n, GitHub, GitLab, Gitea, Vercel, Resend, Notion, Cal.com and WhatsApp; Cal.com month/day/booking details | Configured service accounts |
| Spotify | Windows system media controls | Installed Spotify playback |
| Wardrobe, desktop Mochi, sounds, shortcuts, displays, recap | Existing Windows frontend and native integration | Native visual and interaction checks |
| Phone approval and chat | Android phone link with device authentication | Live Android pairing and remote flow |
| Apple Music | Dedicated integration, commands, state, artwork, per-app volume, pill and voice actions; native track/artwork, play/pause and 5% volume verified | Sustained audible playback and seeking in a session that supports it |
| Crypto prices | Coinbase and OKX public quotes, 24-hour charts, currency selection and opt-in movement alerts | Native settings and pill interaction |
| Portfolio | Read-only exchange balances, 16 native wallet networks, USD valuation and partial-data indicators | Real exchange accounts and each public wallet service |
| Chat microphone and window context | Local dictation fills the editable prompt; selected app/window title and an exposed browser address accompany a new chat | Native microphone, browser navigation-bar detection and window picker click-through |
| Screenshots and annotation | Open native Windows Snipping Tool, capture and annotate there, then explicitly paste a PNG into chat; preview and remove before Send | Installed Snipping Tool discovery is verified; capture, annotation and paste need GUI testing |
| Dropped-file email | Native Windows Share attaches the copied inbox file; the user chooses a mail app and sends there | Mail-client selection and draft attachment on the installed build; native attachment packaging is tested |
| Sixteen languages, including Korean, Japanese, Traditional Chinese and Thai | Complete Windows catalogs, language picker and native locale/plural rules | Native linguistic and layout review |
| Apple iPhone/iCloud features | Windows has Android pairing; no iPhone transport equivalent yet | Cross-platform phone protocol and iPhone implementation |

## Floating toolbar and dictation key

Flow Settings selects the dictation key and the bottom, left or right screen edge. The floating toolbar keeps its buttons in place when tooltips appear; side tooltips open toward the screen interior. Its microphone starts hands-free dictation, the record button toggles Notetaker, and the window button opens Coucou.

The idle island can remain off while explicit shortcuts and tray commands open chat, wardrobe, recap and update views. Background alerts continue to respect the island preference.

Windows offers **Copilot** for keyboards that emit **Win + Shift + F23**, alongside Right Ctrl, Right Alt, Right Shift and Ctrl + Win. Selecting Copilot assigns that chord to dictation while Flow is enabled; other shortcuts pass through. A Copilot key remapped by Windows to Right Ctrl requires selecting Right Ctrl instead. Hardware that emits Win + C is not handled by the Copilot option. Synthetic key injection is ignored. The settings status reports a failed keyboard-listener startup instead of claiming the key is ready. Physical key presses and paste in the target application remain necessary hardware checks.

## Call audio

Choose microphone and playback independently under Flow Settings. A laptop microphone with Bluetooth earbuds is a supported combination. Automatic mode inspects active Windows audio sessions to follow the call application's devices, including selections made inside Zoom or browser calls. If no call is identified, it uses the Windows communications endpoints and then system defaults. Manual choices retain their device identity and never silently switch to speakers. The picker refreshes when devices connect or disconnect.

Automatic detection groups audio processes by their owning application. Multiple microphone applications or multiple outputs within one browser can be ambiguous; Coucou reports this and asks for explicit device choices. It does not read browser tabs or claim to distinguish Google Meet from another audio tab in the same browser. The source meters identify the actual device and the application supplying an automatic route.

A short audio test reports levels without writing audio to disk. It shares source recovery with Notetaker. Device loss, source changes and recoverable WASAPI discontinuities remain visible after audio returns. Recoverable discontinuities do not force a working Bluetooth stream to restart. Detection runs only during a recording or an explicit audio test; Settings polling stops while its window is hidden.

Finished recordings have **Capture details** for each source: device names used, whether audio packets arrived, the highest captured level, the final native format and retained warnings. Peaks include audio between UI updates. Problems expand the details automatically. These details remain after successful transcription removes the audio files. Older notes have only their saved warning; missing diagnostics are not reconstructed. Receiving audio packets does not establish that somebody spoke. A discontinuity means audio may be missing; it does not identify a Bluetooth disconnect or another physical cause.

**Export as Markdown…** opens the native Windows Save As dialog for the summary, key points, to-dos and transcript. Choose the destination and filename there; Windows asks before replacing an existing file. Cancel leaves the note and files unchanged. Export failures remain visible and can be retried. Capture diagnostics stay in the saved note inside Coucou.

The playback source captures the digital mix on the selected endpoint, including other applications on that endpoint. It works with speakers, wired headphones, USB audio and supported Bluetooth endpoints. Recording cannot bypass a device's own bandwidth limits: enabling a Bluetooth headset microphone can change its playback profile. A separate laptop or USB microphone can avoid that limitation. The app shows the active source name and native format, and reports missing or failed sources.

Native checks on the development laptop captured the laptop microphone and Bluetooth earbuds simultaneously at their 48 kHz stereo device format, and the laptop microphone with built-in speakers after the earbuds disconnected. Only aggregate levels were retained. A separate native fixture verified that Windows session detection found an application's actual microphone and playback endpoints. Real Zoom/Meet calls, an in-flight physical disconnect/reconnect and wired/USB hardware remain verification tasks.

Microphone and playback are stored separately at the 16 kHz mono rate required by the speech models. A shared timeline preserves silence and device reconnection gaps. Disk writes run outside the audio callback. Transcript echo filtering removes repeated speaker playback from the microphone transcript; it is not an acoustic noise-cancellation driver. Audio is deleted after transcription succeeds. Summary generation sends text only to the explicitly selected provider.

## Performance evidence

A native check using the installed models completed transcription and automatic calibration with ten seconds of leading and trailing silence, then produced the same original transcript with a manual one-thread setting. Its six-second calibration segment took about 430 ms at one thread, 314 ms at two, 234 ms at four, 653 ms at eight and 1,283 ms at sixteen. A separate call on the installed build selected two threads for its sample. These measurements describe that machine and those recordings, not a universal performance guarantee.

Automatic mode selects a bounded segment containing audio from a successful transcription; leading silence does not prevent measurement. A thread count can win only after repeated matching transcripts. The source transcript stays unchanged. Settings shows the measured choice and retains a manual override. Only timings are stored, never calibration audio.

## Public crypto prices

Enable Crypto under Settings → Integrations, choose Coinbase or OKX, and save up to eight symbols. Requests run once per minute only while the pill is enabled and Coucou is not paused; history is cached for ten minutes. No account is required. OKX dollar pairs are labelled USDT. Missing prices remain unknown, and each failed source shows its error. Alerts are off by default and measure movement since the last alert or first successful quote. Service endpoints and request limits are in `src-tauri/src/markets/defaults.json`; preferences are stored separately in `markets.json`.

## Portfolio

Enable Portfolio under Settings → Integrations. Binance, OKX, Bybit, Kraken and Coinbase Advanced Trade use read-only keys, tested before saving to the OS credential store. Coinbase uses an ECDSA private key. The settings page never reads a saved key back. Disconnect removes that exchange's credentials.

Public wallet addresses support Bitcoin, Dogecoin, Litecoin, Ethereum, Polygon, BNB Chain, Avalanche, Arbitrum, Optimism, Base, Solana, TRON, XRP, Cardano, Cosmos and NEAR. These show native coin balances only; token, staking and derivative positions are not included. Public services receive the address being checked. Addresses and labels live in `portfolio.json`; account balances remain in memory.

Refresh runs only while the pill is enabled and Coucou is not paused. Each account reports its own failures. Missing prices stay unknown, and a partial total is labelled accordingly. USD prices and seven-day charts use public Coinbase endpoints. Service origins and request limits are in `src-tauri/src/markets/portfolio/defaults.json`.

## Apple Music

Apple Music and Spotify have separate pills, commands, event streams and player state. Each talks directly to its own native application and keeps sign-in in that application. Shared code handles only Windows media APIs and the common player layout.

Apple Music volume targets its own Core Audio sessions, including its packaged library agent, across active speaker and headphone outputs. Other players and the system volume are not changed. Playback actions use the selected song or queue in Apple Music. If the application has no playable selection, open Apple Music and choose a song.

Seeking follows the player's advertised capability. The Windows backend rechecks [whether playback-position changes are supported](https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssessionplaybackcontrols.isplaybackpositionenabled) before sending a seek. Unsupported sessions show a read-only progress bar; Coucou does not invent a new position when the player cannot seek. The development laptop's Apple Music session currently reports seeking unavailable.

## Calendar

Cal.com shows upcoming calls in the overview. Details opens a month calendar, then a day and the booking's time, attendee and notes. The date range is bounded to the next sixty days. Cursor pagination follows Cal.com's documented `2026-05-01` API contract, with response and refresh limits in `src-tauri/src/calcom/defaults.json`. Failed or incomplete pages produce an error instead of an apparently complete schedule. The integration is read-only; edits open Cal.com.

## Dropped-file email

After dropping a file, choose Send by email, then Share. Windows opens its native chooser with the copied file attached. Select a compatible mail application, review the draft and send it there. Coucou never sends the message. The native command accepts only a regular file directly inside Coucou's private drop inbox, and its share offer expires after two minutes. Windows Share decides which installed mail applications support the attachment.

## Window attachments

Chat's window button lists visible app titles. Selecting one authorizes a single bounded read of that window's browser navigation bar through Windows accessibility APIs. It never focuses the browser, uses the clipboard or traverses page documents. HTTP(S) addresses are attached only when unambiguous; embedded username/password fields are removed. Unsupported browsers, hidden address bars and unavailable accessibility providers attach the app and title alone. The context chip previews the address before Send, and leaving chat discards a late attachment result.

Ctrl+Alt+F attaches the front window directly on Windows. The native shortcut records the window before opening the island; the selection is single-use and expires after ten seconds. It starts a fresh chat and previews the same context without sending. The shortcut can be changed or disabled in Settings and remains unavailable on Linux.

Dragging Mochi from the island onto an application window selects that window for the same context preview. An unreadable top window blocks attachment instead of selecting one behind it. Dropping on the desktop places Mochi there; moving him once he is on the desktop only changes his position. Native drag targeting and shortcut activation still require GUI verification.

## Screenshots

Chat's screenshot button opens the installed Microsoft Snipping Tool. Capture a region or window, annotate it there if needed, copy it and paste into Coucou's chat field. Win+Shift+S also starts Windows capture. Coucou receives pixels only from the explicit paste event; it does not watch the clipboard or capture the screen in the background.

Pasting a PNG starts a fresh chat with the image attached and shows a preview. Remove attachment clears it before Send; typing or dictating the question does not send it automatically. The image is copied into the private drop inbox with the same seven-day cleanup as dropped files. Limits are shared by the frontend and Rust in `src/chat-image-limits.json`: 5 MiB, 8,192 pixels per dimension and 32 megapixels. Files over these limits are rejected. The workflow uses native capture and annotation rather than an embedded annotation editor.
