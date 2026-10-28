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
| Apple Music | No Windows player integration yet | Implementation and app validation |
| Portfolio and crypto | No Windows port yet | Implementation and read-only account validation |
| Chat microphone and window context | Local dictation fills the editable prompt; user-selected Windows app name and window title accompany a new chat | Native microphone and window picker click-through; browser URL capture is not implemented |
| Traditional Chinese and Thai | Not registered in Windows language picker | Complete translations and language registration |
| Apple iPhone/iCloud features | Windows has Android pairing; no iPhone transport equivalent yet | Cross-platform phone protocol and iPhone implementation |

## Call audio

Choose the actual microphone and the output used by the call application under Flow Settings. Automatic choices use the Windows communications endpoints, falling back to system defaults. Manual choices retain their device identity. A short audio test reports levels without writing audio to disk.

The playback source captures the digital mix on the selected endpoint, including other applications on that endpoint. It works with speakers, wired headphones, USB audio and supported Bluetooth endpoints. Recording cannot bypass a device's own bandwidth limits: enabling a Bluetooth headset microphone can change its playback profile. A separate laptop or USB microphone can avoid that limitation. The app shows the active source name and native format, and reports missing or failed sources.

Microphone and playback are stored separately at the 16 kHz mono rate required by the speech models. A shared timeline preserves silence and device reconnection gaps. Disk writes run outside the audio callback. Transcript echo filtering removes repeated speaker playback from the microphone transcript; it is not an acoustic noise-cancellation driver. Audio is deleted after transcription succeeds. Summary generation sends text only to the explicitly selected provider.

## Performance evidence

A synthetic speech fixture on the development laptop completed transcription and automatic calibration, then produced the same transcript with the manual one-thread setting. The six-second calibration sample took about 613 ms at one thread, 413 ms at two, 370 ms at four, 500 ms at eight and 1,250 ms at sixteen. These measurements describe that machine and fixture, not a universal performance guarantee. Automatic mode measures the user's machine and stores timings, never calibration audio.
