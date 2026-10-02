# Tray, flyout and notifications

On Windows, Coucou lives in the notification area like any other Windows app:
the tray icon says what is going on, a click opens a flyout with every session,
and a Windows notification tells you when a session needs you. The island at the
top of the screen is optional.

## Tray icon

The icon wears a coloured dot for the most important thing going on:

| Dot | Means |
|---|---|
| amber | a session needs you: a question or a permission request |
| red | a session stopped on an error you have not looked at |
| blue | sessions are working |
| green | a session finished and you have not looked at it |
| grey | Coucou is paused |
| none | nothing going on |

Its tooltip names who needs you and where — *Coucou — 3 sessions · web needs you
in Cursor · 1 working*. Windows shows at most 127 characters of it.

Left click opens the flyout; right click opens the menu (Open Coucou, Settings…,
Pause, Quit).

## Flyout

A Windows 11 style panel next to the tray icon, on whichever taskbar edge it is,
inside the screen's work area. It closes when it loses focus or on `Esc`.

- **Header**: Mochi, the number of sessions and how many need you, Pause and
  Settings.
- **Permission card**, when a request is waiting: the session, its host, and
  exactly what Allow authorises (the command, the file, the URL), with Deny and
  Allow.
- **Sessions**: one row per Claude Code session, the ones that need you first —
  project, `bypass` when it runs in bypass mode, state, host and shell, the
  question or last step. Clicking a row (or its arrow) brings that session's
  window to the front (see [terminals.md](terminals.md)); a row you have not
  looked at stays bright until you do.
- **Services**: the integrations you switched on, and whether each one works.
- **Footer**: Ask Claude and Drop a file open the island on the chat or the drop
  zone; Island opens it on the overview.

The island's page owns the sessions and the permission card. It publishes a
snapshot of them; the flyout draws it and sends clicks back. A decision carries
the id of the request it was shown for, so a card that changed under the click
cannot decide a newer request.

## Notifications

A Windows notification is raised when a session:

- asks a question or needs you (button: **Go to Windows Terminal**, **Go to
  Cursor**…);
- stopped on an error;
- finished — only with *Notifications: Also when a session finishes*.

Clicking the button brings the session's window to the front; clicking the
notification opens the flyout. Each session has at most one notification: a new
one replaces the old, and it is taken back from the notification centre once the
session no longer needs anyone. There is never an Allow button on a
notification — it cuts text, and what is approved must be seen whole.

No notification is raised while paused, or when the island already opened a card
for the same thing.

Windows needs an app identity for notifications. The first time one is raised,
Coucou registers its identifier under
`HKCU\Software\Classes\AppUserModelId\fr.louisraille.coucou` with its name and
icon (`%LOCALAPPDATA%\Coucou\coucou.png`). The uninstaller removes both.

## The island

**Settings… → Tray and notifications → Show Coucou in**:

- **Tray only** (default). The island stays folded into an invisible strip that
  lets every click through; it opens only when you ask (Ask Claude, Drop a file,
  Island).
- **Tray and the island at the top** — the island as before: it opens for
  questions, finishes and permission requests, and a notification is raised only
  for what it did not show.

With the island off, a permission request is answered in the terminal unless the
flyout is open at that moment: Coucou declines it at once, Claude Code asks in
its terminal, and a notification points there. If the flyout closes while a
card is up, the same happens. Claude Code never waits on a card nobody can see.

## How this is tested

Automated (`cargo test --workspace`, `npm test`): the flyout's place for all four
taskbar edges, a second display with negative coordinates, the tooltip limit,
the tray badges, the click that closes the flyout not reopening it, the
notification document (escaping, the button, no Allow), what a notification
click may send back, the tray state and tooltip text, the flyout's rows, the
snapshot, and when a notification is raised or taken back.

End to end, on Windows 11 at 125 % with the taskbar at the bottom, on a separate
instance (see [terminals.md](terminals.md#how-this-is-tested)):

- the tray icon, found in the notification area overflow with UI Automation,
  carries the snapshot's tooltip, and invoking it opens the flyout next to it,
  inside the work area;
- relay events fill the flyout's rows; a question raises a notification whose
  content (title, text, button and its session) is read back from the
  notification history;
- with the flyout open, a permission card shows exactly what is authorised and
  Deny reaches Claude Code's hook; with it closed, the request goes back to the
  terminal in about 50 ms with a notification; closing the flyout with a card up
  hands it back;
- answering a question takes its notification back;
- with the island switched on, a question opens the island and raises no
  notification; switched off, the island folds into a strip that lets clicks
  through.

A notification's button cannot be clicked by automation (the notification UI is
not exposed to it); that a click reaches Coucou with the button's arguments was
checked by hand.
