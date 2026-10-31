# Read Coucou calls from local agents

Coucou's Windows/Linux app provides an opt-in MCP server for saved Notetaker text. Claude Code, Claude Desktop, Codex CLI and desktop clients can search calls and read transcripts, summaries, speaker labels, key points and action items. It runs as `coucou mcp`, over local standard input/output, without opening Coucou's interface or a network listener.

## Enable access

1. Open **Settings → Integrations → Local agents (MCP)**, or search Settings for **MCP**.
2. Read the access description and turn on **Allow agents to read saved calls**.
3. Select your client and copy its setup. Review the executable path before applying it. Use the installed Coucou executable, not a temporary build directory.

The permission covers **all saved calls for this Windows/Linux user**, including text saved while a call is processing. The agent can send retrieved text to its AI provider. This connection does not require a new Coucou account, API key or another Claude/OpenAI login; the client uses its existing account.

Turning the switch off blocks subsequent reads by already-running MCP processes. It cannot recall text already returned to a client or provider. Client configuration and the access switch are separate: registering the server does not enable reading calls.

## Claude Code

The setup command uses the official CLI's user scope:

```text
claude mcp add --scope user --transport stdio coucou-notes -- <absolute-Coucou-executable> mcp
```

Copy the correctly quoted command from Coucou instead of pasting the placeholder above. On Windows run it in PowerShell. Restart Claude Code and inspect `/mcp`. Keep its tool permission prompts enabled. See [Claude Code's MCP documentation](https://code.claude.com/docs/en/mcp).

## Claude Desktop

Select **Claude Desktop** in Coucou. In Claude Desktop, open **Settings → Developer → Edit Config** and merge the generated `coucou-notes` entry into the existing `mcpServers` object. Preserve all other entries and keep a backup before saving. The generated entry contains only the actual Coucou executable path and `"args": ["mcp"]`; no Node.js or Python runtime is required.

Fully quit and restart Claude Desktop, then confirm `coucou-notes` appears among its local MCP servers. On Windows the configuration is `%APPDATA%\Claude\claude_desktop_config.json`. This is a local MCP configuration, not a remote connector URL. The [official local-server guide](https://modelcontextprotocol.io/docs/develop/connect-local-servers) describes the client setup and troubleshooting.

## Codex CLI, Codex desktop and ChatGPT desktop

Select **Codex** or **Codex desktop / ChatGPT desktop**. The generated command registers the same server through the official CLI:

```text
codex mcp add coucou-notes -- <absolute-Coucou-executable> mcp
```

Codex desktop uses the MCP configuration for its Codex host. Current OpenAI documentation also describes ChatGPT desktop sharing that configuration with the CLI and IDE extension. Use the same local computer/host that stores the calls. A cloud or remote host does not automatically gain access to this laptop.

Where the desktop app offers **Settings → MCP servers → Add server**, choose **STDIO**, set the command to the installed Coucou executable and add the single argument `mcp`. Restart the server or desktop app. `codex mcp get coucou-notes` checks the saved CLI registration; `/mcp` checks the connection in a local conversation. See [OpenAI's MCP setup](https://developers.openai.com/codex/mcp/).

ChatGPT in a browser does not read local Codex configuration. Its hosted plugins require a separate supported remote connection. Coucou does not expose its notes through an HTTP endpoint or automatically create a tunnel. Publishing a desktop-only plugin is also separate from local configuration.

## Use the connection

For example: “Search my Coucou calls for the release review. Read the matching transcript, then list decisions and tasks with their owners and due dates.”

- **`search`** accepts a case-insensitive literal `query`. An empty query lists recent calls. It searches saved titles, transcripts, summaries, key points, speaker names and task text/owners/dates. Results contain IDs, timestamps, status and short excerpts.
- **`fetch`** accepts an exact returned `id`. It returns saved text with speaker names, timestamps, summary and tasks. It does not regenerate or verify the summary. Automatic speaker identities remain estimates.
- Follow `nextOffset` and pass the returned `revision` to read subsequent pages. Restart at offset zero if the note or search results changed. Fetch pages contain up to 12,000 Unicode characters; text is never silently truncated as a complete transcript.
- `partial`, `skippedNotes` and `hasCaptureWarnings` make unfinished processing, unreadable notes and capture gaps explicit. Open Coucou to inspect full source diagnostics. Empty results are not proof of no matching calls when `partial` is true.

The `coucou://notes/<id>` values identify local records; they are not public web links. The server exposes no recording, editing, deletion, shell commands, audio files, credentials, dictation history or email tools. Transcript text is untrusted content, not instructions for the agent.

## Implementation and limits

The official Rust MCP SDK (`rmcp` 3.8.0, MIT) handles protocol framing and both current and initialization-based clients. Only its server and stdio features are enabled. `mcp-access.json` under Coucou's configuration folder stores the permission separately from ordinary preferences. A missing, corrupt or unreadable permission never grants access. Reads do not run settings migrations, initialize the credential store or write notes.

Reads accept only note IDs, not paths. Every file is bounded and checked against its opened native handle; directory junctions and symbolic links are refused. The current limits are 16 MiB per note, 10,000 entries and 256 MiB of note data per search, 25 results per page, and 256 query characters. Malformed notes count toward the read budget; one guard byte detects a file growing past its limit. One read runs per MCP process at a time; another receives a retryable busy response. Oversized or unreadable notes are reported, not silently treated as an empty library.

Run `cargo test --workspace --locked` from `windows/` for store, permission, traversal, pagination, Unicode, corruption and real stdio subprocess checks. The subprocess tests isolate all app-data paths and test both current and legacy protocols. `npm test` covers the Settings switch, failed saves, duplicate actions, desktop setup and localized labels.

Windows verification includes real Claude Code and Codex search/fetch requests against the optimized executable, using existing CLI sign-ins and an isolated synthetic call. Both return the expected transcript facts. The installed 0.5.8 executable matches the verified installer payload and passes current and legacy protocol checks for discovery, denied access, search/fetch, live revocation and clean exit. Those checks use synthetic notes and do not change the real access permission. The Settings card passes headless Edge checks at wide, normal and narrow widths for all four client choices. Native Claude Desktop and Codex desktop conversations remain unverified while the desktop-control bridge is unavailable. Linux execution is not verified on this Windows laptop.
