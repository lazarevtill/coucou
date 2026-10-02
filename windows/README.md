<div align="center">

<img src="src-tauri/icons/128x128.png" width="96" alt="Coucou icon">

# Coucou for Windows

**Mochi doesn't get a notch on a PC — so it lives in your notification area instead.**

See every Claude Code session at a glance, jump to the one that needs you, approve permissions, drop a file, chat with Claude, keep an eye on your services — without leaving what you're doing.

![Windows 10/11](https://img.shields.io/badge/Windows-10%2F11-0078D4?logo=windows)
![Tauri 2](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=black)
![Rust](https://img.shields.io/badge/Rust-backend-000?logo=rust)
![License: MIT](https://img.shields.io/badge/license-MIT-green)

</div>

<img src="screenshots/greeting.png" width="640" alt="Mochi waving hello at launch">

---

## Install

The downloadable installer is **temporarily unavailable**. Microsoft Defender
wrongly flags the unsigned installer as malware (`Trojan:Win32/Wacatac.H!ml`, a
machine-learning false positive). A report is under review at Microsoft, and the
installer will be published again once it is cleared and code-signed.

Until then, [build it yourself](#build-it-yourself): it takes a few minutes and
installs for the current user only — no admin prompt.

## Using it

<img src="screenshots/compact.png" width="292" alt="The compact island, with the integration pills as mini Mochis">
<img src="screenshots/overview.png" width="640" alt="The overview: the focused integration on the left, the other pills on the right">
<img src="screenshots/approval.png" width="640" alt="A Claude Code permission request, with Deny and Allow">
<img src="screenshots/chat.png" width="640" alt="Chatting with Claude from the island">
<img src="screenshots/drop.png" width="640" alt="Mochi turned into a box, waiting for a file">

| What you do | What happens |
|---|---|
| Click the tray icon | The flyout: your sessions, permission requests and services |
| Click a session, or **Go to Windows Terminal** / **Go to Cursor** on a notification | The window that session runs in comes to the front |
| Click the file under a session in the flyout | Its latest change, line by line, with its last steps |
| Move the mouse to the very top-centre of the screen (island on) | Mochi peeks out |
| Click the small island | It opens |
| Click Mochi | It gets annoyed. Three times in a row and it goes dizzy |
| Rest the pointer on Mochi for two seconds | Hearts |
| Drag a file onto the island | Mochi turns into a box, swallows it, then offers to answer questions about it |
| `Esc` | Closes the island |
| Right-click the tray icon | Open, Settings…, Pause, Quit |

Everything else happens on its own: the tray icon's dot and tooltip say what is
going on, a question raises a Windows notification that takes you to the session
asking it, and a permission request shows **Deny / Allow** with exactly what it
authorises. The island at the top of the screen is optional (**Settings… → Tray
and notifications**); how the tray, the flyout and the notifications behave:
[docs/shell.md](docs/shell.md).

## Claude Code

<img src="screenshots/settings.png" width="562" alt="The settings window">

Open **Settings… → Claude Code → Install hooks…**. You get the exact diff of what
will change in `%USERPROFILE%\.claude\settings.json`, the path of the dated backup
that will be taken, and nothing is written until you click. Your own hooks are
never touched, and uninstalling removes only Coucou's entries.

The relay is a tiny executable, `coucou-hook.exe`, copied to
`%LOCALAPPDATA%\Coucou\bin\` at launch. It is given 300 ms to reach Coucou and
exits cleanly if the app is closed, slow or crashed — **a Claude Code session is
never blocked or slowed down by Coucou.** If nobody answers a permission request
in time, Coucou stays quiet and Claude Code asks in the terminal as usual.

It works from any terminal — Windows Terminal, PowerShell, Cursor, VS Code, Git
Bash — and with any number of sessions at once. The Claude Code pill counts them
and shows the one that needs you first, with where it runs and whether it is in
bypass mode. How Coucou tells sessions apart and finds their windows:
[docs/terminals.md](docs/terminals.md).

**Settings… → Open projects in** picks what the island's buttons open: Cursor,
VS Code or nothing for the editor; Windows Terminal, a PowerShell window or
nothing for the terminal. A choice that isn't installed falls back to File
Explorer.

## Chat and keys

The chat answers with Claude (**Settings… → Claude** takes your Anthropic API
key) or with your own model server — llama.cpp's `llama-server`, LM Studio,
Ollama or anything else that speaks the OpenAI chat API (**Settings… → Chat**,
details in [docs/model-sources.md](docs/model-sources.md)). Keys live in the
**Windows Credential Manager**, never on disk and never in the interface — the
island can only ask whether a key exists. Same for every integration key.

**Plugins** add status sources of your own — a web address read on a timer, or an
MCP server — next to the built-in integrations. Nothing runs until you review it
in **Settings… → Plugins** and switch it on: [docs/plugins.md](docs/plugins.md).

No telemetry. The only network requests Coucou makes are to the services you
configure yourself.

## Build it yourself

You need [Rust](https://rustup.rs), [Node 20+](https://nodejs.org), and the
**MSVC build tools** (Visual Studio Build Tools with "Desktop development with
C++"). WebView2 ships with Windows 10/11.

```powershell
cd windows
npm install
npm run tauri dev      # live-reloading development build
npm run pack           # builds the installer and drops it in windows/release/
```

`npm run dev` alone serves the front end in an ordinary browser, which is enough
to work on the island's looks. It also serves `dev/upload-preview.html`, which
replays the whole file-drop choreography on a loop — the one part of the UI that
otherwise needs a real drag from Explorer to see. Neither page ships in the app.

`npm run pack` leaves two files in `windows/release/`, the same names the release
workflow publishes:

```
Coucou-Windows-X.Y.Z-setup.exe    the versioned installer
Coucou-Windows-setup.exe          the same file under the rolling name
```

Installing is optional — `target/release/coucou.exe` runs on its own. There is no
window in the taskbar and no console: the island at the top of the screen and the
Mochi in the notification area are the whole app, and Quit lives in its menu.

The 28 sounds are the macOS app's own files; they are never duplicated in this
folder. The path is declared once, in `SOUNDS_DIR` at the top of
`vite.config.ts` — when they move to `shared/sounds/`, change that one line.

The app icon and the tray icon are drawn in code, like Mochi itself:

```powershell
npm run icons          # regenerates src-tauri/icons from scripts/gen-icons.mjs
```

### Layout

```
windows/
  src/                 island front end (TypeScript, no framework)
    mochi/             Mochi and the launch greeting, in Canvas 2D
    island/            state machine, hooks, integrations
    views/             every island view
    settings/          the settings window
    flyout/            the tray flyout
  src-tauri/           Rust backend: window, named pipe, Claude API, pollers
  hook/                coucou-hook.exe, the Claude Code relay
  docs/                how parts of the app work, in depth
  tests/               front-end logic tests (`npm test`)
  scripts/             icon generator
```

`cargo test --workspace` runs the Rust tests, `npm test` the TypeScript ones
(Node 22.18 or newer: the tests load the TypeScript sources directly).

### Log

`%LOCALAPPDATA%\Coucou\coucou.log` — hook events, permission decisions, jumps to
a session's window (the host and how it went, no folder names), poller problems.
It stays on your machine.

## What's different from the Mac version

- No notch: Coucou lives in the notification area, with a flyout and Windows
  notifications. The island is optional; switched on, it sits at the top centre
  of the screen and retracts into the top edge.
- Permission approval works from **any** terminal; the Mac build only listens to
  VS Code sessions.
- Each Claude Code session is followed on its own, and a question takes you to
  the exact Windows Terminal window, or the Cursor or VS Code window of the
  session's workspace. Tabs are not switched (see
  [docs/terminals.md](docs/terminals.md)).
- Not in this version: sending a file by email, and dragging Mochi onto a window
  to attach it as context.
- Cal.com shows the next bookings as a list rather than the Mac's calendar.

## Linux

The same app builds for Linux: everything that differs lives in
`src-tauri/src/platform/`, and the relay's transport in `hook/src/unix.rs`.

```bash
sudo apt install build-essential pkg-config \
  libwebkit2gtk-4.1-dev libgtk-layer-shell-dev libayatana-appindicator3-dev \
  librsvg2-dev libssl-dev libdbus-1-dev patchelf \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good
npm install
npm run tauri dev      # live-reloading development build
npm run pack           # AppImage, .deb and .rpm in windows/release/
```

What changes on Linux:

- **The island** is a gtk-layer-shell overlay anchored to the top edge, over any
  top panel, on compositors that support it: COSMIC, KDE Plasma, Hyprland, Sway
  and other wlroots compositors. GNOME has no layer-shell, so there the island
  is a regular window. `COUCOU_LAYER_SHELL=0` forces that mode anywhere.
- **Click-through** is the window's input region, kept equal to the island
  shape, so the compositor sends every other click to what is underneath.
- **Mochi's eyes** follow the pointer only while it is over the island: Wayland
  gives no app the cursor position anywhere else.
- **Claude Code hooks** go through `~/.local/share/coucou/bin/coucou-hook` and a
  Unix socket at `$XDG_RUNTIME_DIR/coucou.sock`. Both ends check that the other
  runs as the same user.
- **Keys** live in the Secret Service (GNOME Keyring, KWallet).
- **Files**: preferences in `~/.config/coucou/`, the log at
  `~/.local/share/coucou/coucou.log`.
- What the Windows build leaves out, this one does too: sending a file by
  email and dragging Mochi onto a window. Jumping to a session's window is not
  available either — Wayland lets no app raise another's window. The editor
  button opens Cursor or VS Code when it is on your `PATH`; the terminal button
  opens the folder in the file manager.
