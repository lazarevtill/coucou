# Plugins

Plugins are status sources you add yourself, next to the built-in integrations:
a web address read on a timer, or an MCP server whose read-only tool is called on
a timer. What they find shows in the tray flyout's **Services** list; a click
opens the plugin's link.

Nothing runs until you review a plugin and switch it on in **Settings… →
Plugins**, and a plugin whose manifest changes is switched off until you look at
it again.

## Where they live

`%APPDATA%\Coucou\plugins\<id>\plugin.json` — one folder per plugin, named after
its id. **Settings… → Plugins → Open the plugins folder** opens it; **Look
again** re-reads it.

## plugin.json

```json
{
  "schema": 1,
  "id": "github-status",
  "name": "GitHub status",
  "version": "1.0.0",
  "color": "#24292F",
  "kind": "http",
  "pollSecs": 120,
  "allowedHosts": ["www.githubstatus.com"],
  "http": { "url": "https://www.githubstatus.com/api/v2/summary.json" },
  "show": { "headline": "/status/description", "items": "/components", "title": "/name", "detail": "/status" },
  "links": [{ "label": "Open status page", "url": "https://www.githubstatus.com" }]
}
```

| Field | |
|---|---|
| `schema` | `1` |
| `id` | lowercase letters, digits and hyphens, at most 40; the folder's name |
| `name`, `version` | plain text, at most 40 and 20 characters |
| `color` | `#RRGGBB`, for the dot in the list |
| `kind` | `http` or `mcp` |
| `pollSecs` | how often it is polled; 30 s to a day, 5 min if absent |
| `secrets` | `[{ "key": "token", "label": "API token" }]` — at most 8; keys in `a-z 0-9 _` |
| `allowedHosts` | every host the plugin may talk to, exactly: no wildcards, schemes or ports |
| `allowPrivateNetwork` | `true` to reach this computer or the local network |
| `http` | `{ "url": …, "headers": { … } }` |
| `mcp` | see below |
| `show` | JSON Pointers (RFC 6901) into the answer: `headline`, `items` (a list), and per item `title` and `detail` |
| `links` | up to 4 https links; the first opens from the flyout |

Unknown fields are refused, so a typo cannot quietly switch a safety setting off.

### Secrets

Declare them in `secrets` and use them as `{{secret.<key>}}` in an HTTP header or
an MCP server's environment. Their values are typed into **Settings… →
Plugins** and stored in the Windows Credential Manager as
`plugin:<id>:<key>`; a plugin can reach only its own. A secret can never go into
a URL or a command line, where other programs and logs could see it.

### An MCP server

```json
"kind": "mcp",
"secrets": [{ "key": "api_key", "label": "API key" }],
"mcp": {
  "transport": "stdio",
  "command": "C:\\Program Files\\nodejs\\node.exe",
  "args": ["C:\\tools\\tickets\\server.js"],
  "env": { "TICKETS_KEY": "{{secret.api_key}}" },
  "poll": { "tool": "open_tickets", "arguments": { "limit": 5 } },
  "tools": { "open_tickets": "read", "close_ticket": "write" }
},
"show": { "headline": "/summary", "items": "/tickets", "title": "/title", "detail": "/state" }
```

- `transport`: `stdio` (Coucou starts the server) or `http` (`"url"`, and
  `"headers"`, for a server already running; its host goes in `allowedHosts`).
- `command`: an absolute path to an `.exe`. Nothing is looked up on `PATH`, and
  shells, batch files and script hosts are refused — they would run whatever
  they are given.
- `tools`: what each tool does, in your words: `read` or `write`. A server's own
  description of its tools is not trusted.
- `poll`: the one tool called on a timer, which must be marked `read`. Coucou
  never calls any other tool.
- `show`: picked out of the tool's structured result, or its text read as JSON;
  without `show`, the first line of its text.

Coucou speaks MCP `2026-07-28` and falls back to `2025-11-25` for servers that
need the `initialize` handshake, on both transports.

## What Coucou enforces

- **Approval.** A plugin is off until you tick *I reviewed this and trust it*
  and switch it on. The approval stores the SHA-256 of the exact `plugin.json`
  you saw; any change to the file switches the plugin off (within a minute) until
  you review it again.
- **Network.** Everything an HTTP plugin or an MCP-over-HTTP plugin sends goes
  through one guard:
  - only the hosts in `allowedHosts`; each one is resolved once and pinned to
    those addresses, so a changed DNS answer cannot point it elsewhere;
  - https only — plain http only with `allowPrivateNetwork`, and then only to
    addresses on this computer or the local network;
  - an address on the local network (including `100.64.0.0/10`, which Tailscale
    uses) only with `allowPrivateNetwork`;
  - every redirect checked by the same rules; no system proxy (it would resolve
    names itself);
  - 15 seconds and 1 MB per answer.
- **Programs** (`stdio`). The server is started without a shell, with only what a
  runtime needs from the environment (`SystemRoot`, `PATH`, `TEMP`, the profile
  folders) plus the plugin's `env`, and without a console window. It runs for
  one poll and is ended afterwards: its input is closed, it gets two seconds to
  exit, then it is stopped. On Windows it is in a job object that ends it, and
  anything it started, when its session ends or Coucou quits — a process it
  starts in the instant before it is put in the job keeps running. A program
  can do anything you can on this computer; the network rules above do not
  reach inside it.
- **Pause.** While Coucou is paused nothing is polled.
- **Failures.** A plugin that fails waits twice as long before each new try, up
  to an hour.
- **Log.** `coucou.log` records that a plugin was approved or failed, never what
  it read.

## Built-in integrations

Stripe, GitHub, Vercel, n8n, Resend, Notion and Cal.com run on the same
mechanism, with their own cards. They are switched on in **Settings… →
Integrations**, up to four at a time, as before.

Built-ins and plugins share one setting, `plugins` in `settings.json`. A
`settings.json` from before it — with `activeIntegrations` — is moved over on
start: the same integrations stay on, and the old file is kept next to the new
one as `settings.json.<date>-<time>.bak` the first time it is rewritten.

## How this is tested

Automated (`cargo test --workspace`, `npm test`):

- the manifest rules, every refusal with its reason; SHA-256 against the NIST
  vectors;
- the guard against an HTTP server started in the test: allowed and refused
  hosts, a name resolving to a local address, redirects within and off the
  allowlist, an oversized answer;
- MCP: messages and replies, version errors, the event-stream reader, the
  header encoding (checked against the specification's examples), the stdio
  connection over canned lines, and streamable HTTP against a test server —
  current, current-refusing-the-version, and older with `initialize` and its
  session id;
- real server processes (`src-tauri/tests/fixtures/mcp-test-server.mjs`, Node):
  a current server, an older one that floods stderr and pings, one that exits on
  the probe and has to be restarted; the declared secret reaches the server and
  nothing else of Coucou's environment does; the server is gone when its session
  ends;
- the plugins folder, approvals and changed manifests; the secrets namespace;
  migrating `activeIntegrations`, including the dated backup.

End to end on a separate instance (see
[terminals.md](terminals.md#how-this-is-tested)), with an old-format
`settings.json`:

- an HTTP plugin reading GitHub's public status page, an MCP plugin running the
  test server with a secret, and a broken manifest: the broken one is listed
  with its reason; *Switch on* stays disabled until the box is ticked;
- switching the HTTP plugin on stored the hash of the file, rewrote
  `settings.json` without `activeIntegrations`, and kept the dated backup; seconds
  later the flyout showed *GitHub status — All Systems Operational*;
- the MCP plugin, with its secret saved in the settings window, showed *Tickets —
  3 open tickets*; its server's tools were listed; no server process was left
  running between polls;
- editing the approved manifest switched the plugin off within a minute, with
  *Changed since you approved it*.

Not covered end to end: Pause stopping plugin polls, an MCP server over HTTP
other than the test server, Linux.
