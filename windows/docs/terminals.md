# Sessions, terminals and jumping to a window

Coucou follows every Claude Code session on its own, says where each one runs,
and brings the window of a session to the front when you ask — most usefully
when a session is waiting for you.

## One record per session

Every hook event carries Claude Code's `session_id`. The island keeps one record
per session (`src/core/sessions.ts`): its state, its project, its last steps,
what it is waiting for, its host and its permission mode. The Claude Code pill
shows their sum (`src/core/sessionView.ts`):

- the pill reads **Claude Code · 3** with three sessions;
- the session that needs you comes first: a permission card, then a question,
  then an unseen failure, then work in progress, then unseen finishes;
- the grey line names the leading session's host, the number of sessions and
  `bypass` when it runs in bypass mode;
- a finish or a failure stays "unseen" until you open the Claude Code pill, jump
  to it, or press **OK** on its card.

A session leaves the list on `SessionEnd`, or when its `claude.exe` is no longer
running (checked every 20 seconds: a closed terminal or a crash sends no
`SessionEnd`), or after six hours without an event.

## Waiting for you

A session is waiting when:

- it calls the `AskUserQuestion` tool. Claude Code announces this with
  `PreToolUse` and then — even in bypass mode — a `PermissionRequest`. A question
  is not a permission: Coucou declines that request at once, so the terminal shows
  the question straight away, and the island shows a question card instead of
  Allow / Deny;
- a `Notification` says a human is needed (`permission_prompt`,
  `elicitation_dialog`, `elicitation_url_dialog`, `agent_needs_input`). Claude
  Code follows a question with a generic "needs your permission" notification;
  a session that is already waiting keeps the more precise text;
- a permission request could not be shown (another card was up) or was not
  answered in time: it is then asked in the terminal, and the session waits
  there.

`idle_prompt` is not waiting: a session sitting at its prompt after finishing is
just finished. Waiting ends with the next prompt, tool call or stop.

The question card says what the session asks and where — *asks in Windows
Terminal* — with **Go to Windows Terminal** (or **Go to Cursor**…). Opening the
island while a session waits shows that card first. The card goes away by itself
once nobody waits any more.

## Where a session runs

`coucou-hook.exe` adds to each event, computed on the spot and never taken from
the payload:

| Field | What it is |
|---|---|
| `host_chain` | the processes above the relay, nearest first: pid and executable **file name** only (`bash.exe`, `claude.exe`, `powershell.exe`, `WindowsTerminal.exe`), at most 12, stopping at Windows' own processes |
| `host_window` | the window that owns the session's console, when there is one (see below) — a bare window handle |
| `host_hint` | `cursor` or `vscode` when the terminal belongs to that editor family, from `TERM_PROGRAM` and the *shape* of `GIT_ASKPASS` — the path itself is never sent, it contains the user name |
| `ide_port` | Claude Code's IDE-integration port, `CLAUDE_CODE_SSE_PORT` |

It removes `transcript_path` and `tool_response`: a tool's result can be a whole
file read or a command's whole output. One part comes back, for Edit and Write
only: Claude Code's own patch of the change (`structuredPatch` — the changed
lines with a little context and their line numbers, at most 10 hunks and 120
lines, `truncated` when cut) and whether Write created the file. That is what
the flyout's code view shows. The file as it was before the change
(`originalFile`) never leaves the relay. Every string is cut at 2,000 bytes.

The app (`src-tauri/src/host.rs`) decides the host from the chain first —
`Cursor.exe`, `Code.exe`, `WindowsTerminal.exe` — because environment variables
are inherited and lie: a Windows Terminal started from Cursor still says
`TERM_PROGRAM=vscode`. Then the IDE lock file, then the hint, then
`TERM_PROGRAM` and `WT_SESSION`. The shell (Windows PowerShell, PowerShell 7,
cmd, bash) is the nearest shell above `claude.exe`.

`~/.claude/ide/<ide_port>.lock` is written by Claude Code's IDE integration and
names the editor, its main process and its workspace folders. Its format is not
documented, so it is read once per session as a hint only, and its `authToken`
is never read into memory.

## Jumping to the window

**Windows Terminal** runs all of its windows in a single process, so a process id
cannot tell them apart. Each tab, though, hosts its programs on a pseudo console
whose hidden window is owned by the terminal window the tab sits in. The relay
attaches to the session's console for an instant (`hook/src/console.rs`), reads
that owner, and detaches; while attached it ignores Ctrl+C, and it puts its own
standard handles back afterwards. The jump then lands on exactly that window.

**Cursor and VS Code** leave their pseudo consoles unowned, and one editor
process owns every editor window. Coucou takes the editor's main process from
the lock file and the chain, and picks the window whose title contains the
workspace folder or the session's folder name; with no match, the editor's
front-most window.

Before anything is focused, the app checks again: an exact window must still be a
visible window of a known terminal or editor process, and a process must still
be running the executable it was recorded with — window handles and process ids
are recycled.

Windows only lets the app that received your click change the foreground
window. Coucou tries in order, checking the result after each step:

1. restore the window if it is minimised, then ask for the foreground;
2. if that did not take, attach to the foreground window's input queue for the
   length of one call, and ask again;
3. if that did not take either, flash the window's taskbar button, and the
   island says so.

No key presses are simulated.

What a jump cannot do:

- switch to a **tab** inside a Windows Terminal window — the terminal has no
  public way to do it; you land on the right window;
- reach a session with no window: the Claude desktop app, a browser extension,
  a session started without a terminal. The island says there is no window;
- pick between several windows of one editor that have the same folder name.

When the app does not know a session (an older relay without `session_id`), ↗
opens the project in the editor instead.

## Open projects in

**Settings… → Open projects in** decides what the island's buttons start:

| Setting | Starts |
|---|---|
| Editor: Cursor | `cursor.cmd` from `PATH`, else Cursor's per-user install |
| Editor: VS Code | `code.cmd` from `PATH` — skipping Cursor's copy, which is also called `code` — else VS Code's per-user install |
| Terminal: Windows Terminal | `wt.exe -d <folder> <shell>` |
| Terminal: PowerShell window | the shell in a new console window, in the folder |
| None | nothing |

The shell is PowerShell 7 (`pwsh.exe`) when installed, else Windows PowerShell.
Nothing goes through `cmd /C`. The folder must be an absolute, existing
directory; Windows Terminal reads `;` as a command separator, so a folder with
one in its name opens in File Explorer instead. File Explorer is also what opens
when the chosen program is missing or fails to start.

## How this is tested

Automated:

- `cargo test --workspace` — host classification on process chains copied from
  real Cursor and Windows Terminal sessions, the lock-file reader, the session
  table, window choice, launch plans, the console target, and the relay itself
  run against a private named pipe (`hook/tests/relay.rs`);
- `npm test` — the session model and everything the island derives from it
  (`tests/*.test.mjs`).

End to end, on Windows 11 with Windows Terminal 1.24, Windows PowerShell 5.1,
Cursor 3.23 and Claude Code 2.1.287, without touching the installed Coucou:

1. Build a separate instance under its own identifier, so it cannot meet the
   installed one: write `{ "identifier": "fr.louisraille.coucou.e2e" }` to a
   file, then `npx tauri build --debug --no-bundle --config <that file>`.
2. Run `target\debug\coucou.exe` with `APPDATA` and `LOCALAPPDATA` pointing at a
   scratch folder (its settings, log and relay copy go there), `COUCOU_PIPE` set
   to a private pipe name, and integrations switched off in its settings.
3. Start Claude Code in a new Windows Terminal window with
   `COUCOU_PIPE` set and
   `claude --setting-sources project --settings <hooks.json> --permission-mode manual "<prompt>"`,
   where `hooks.json` holds the same hooks as the installer, pointing at the
   scratch relay. `--setting-sources project` keeps your own user hooks — and so
   your installed Coucou — out of it.
4. Prompts used: one that calls `AskUserQuestion` (question card, no Allow/Deny,
   question kept after the follow-up notification, **Go to Windows Terminal**
   lands on that session's window and not another window of the same terminal);
   one that writes a file outside the folder (permission card naming the file,
   **Deny** reaches Claude Code and nothing is written, then the finished card).
5. For Cursor, a relay started outside any terminal with the IDE port of a
   running Cursor session (question card says *asks in Cursor*, **Go to Cursor**
   brings that workspace's window forward).
6. Kill a test session's `claude.exe`: its card and record go once the
   20-second liveness check sees it.

Not covered end to end: a session typed into Cursor's own terminal, several
Cursor windows open at once (covered by the window-choice tests), Linux.
