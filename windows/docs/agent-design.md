# Local agent — design

**Design only. None of this is built.** It records how a "Dots-style" assistant
would sit on top of what Coucou already has, so it can be built in reviewed
steps later.

## What it is for

A small assistant with a name and a memory that, on its own schedule, looks at
what Coucou already sees (sessions, integrations, plugins) and says something
useful — a digest, a question — and that you can ask to hand a task to Claude
Code in a project.

## Building blocks that exist

- **Chat sources** ([model-sources.md](model-sources.md)): Claude or your own
  OpenAI-compatible server. The agent picks a source per role: `chat`,
  `proactive` (can be a local model, so routine checks never leave the machine)
  and `reviewer` (a cheap model).
- **Plugins** ([plugins.md](plugins.md)): status from HTTP endpoints and MCP
  servers, read on a timer, already approved by you with their hosts, secrets and
  tools.
- **Sessions** ([terminals.md](terminals.md)): every Claude Code session, its
  state, project and window.
- **Notifications and the flyout** ([shell.md](shell.md)) to show what it says.

## Safety model (enforced in code, never by the model's obedience)

1. **Two kinds of run.** A *proactive* run (scheduled, nobody watching) is built
   with a tool registry that physically contains only read tools and the agent's
   own memory; write tools are never constructed, never shown to the model, and
   the dispatcher re-checks access before any call. A *requested* run (you asked)
   may propose write actions, which go through the rules below.
2. **Rules.** `rules.json`: `{ scope, tool, action: allow | ask | block }`.
   Resolution order: a hard-coded never-list (credentials, payments, deletions,
   account and security settings — exact ids for built-ins, name patterns for
   MCP tools, documented as patterns) › your blocks › the most specific rule ›
   the default (read: allow, write: ask). "Always allow" creates a visible,
   editable row.
3. **Review.** Before an `ask` reaches you, a cheap model sees the instruction,
   the proposed call (secrets redacted) and the matching rule, and answers in
   strict JSON. It can only make things stricter (allow → ask → block), never
   looser; an error or a timeout escalates to you.
4. **Budget.** A spend cap and a ledger (prices as an editable estimate); a
   runs-per-hour limit in code; a kill switch in the tray (*Pause agent*) that
   stops everything at once — the same gate plugins and integrations obey.
5. **MCP tool descriptions are untrusted.** A tool's access comes from your
   approved plugin manifest, never from the server's annotations.
6. **No prompt, transcript or file content of a Claude Code session goes to the
   agent** — only state, project name, host, elapsed time and current tool.

## Memory

`%APPDATA%\Coucou\agent\`: `profile.json` (name), `notes.md`, `preferences.json`,
`rules.json`, `activity.jsonl` (every proposed and taken action), `budget.json`.
*Forget everything* deletes the folder. A dry run shows exactly what would be
sent before anything is.

## Delegating to Claude Code

You give a task and a project folder (from the sessions Coucou knows, or a path).

- **Visible first.** Coucou opens a Windows Terminal tab in the folder running
  `claude`, with the task written to `%APPDATA%\Coucou\agent\tasks\<id>.md` and
  read by a fixed script — no user text on any command line (Windows Terminal
  reads `;` as a separator, and command lines are visible to every program).
  The tab carries `COUCOU_TASK_ID`, which the relay forwards, so the session is
  linked by id, not by guessing from process ids. Permission requests use the
  existing card.
- **Permission mode pinned on the command line.** Only `--permission-mode
  manual` or `plan` (Claude Code 2.1.287: `acceptEdits`, `auto`,
  `bypassPermissions`, `manual`, `dontAsk`, `plan`). A user setting such as
  `defaultMode: dontAsk` would otherwise be inherited. Tests assert that no
  generated command line contains `bypass`, `dontAsk`, `auto`, `acceptEdits` or
  `dangerously`.
- **Report back**: the session's last message, `git diff --stat`, and a pull
  request URL read with `gh pr view` (read-only).
- **Headless later, after a spike.** In `claude -p` the `PermissionRequest` hook
  does not fire, so approvals would need `--permission-prompt-tool` with a small
  MCP server (possibly a mode of `coucou-hook.exe`). Its contract is not in the
  CLI reference and must be checked against the installed `claude` first.

## Steps, each reviewed on its own

1. **Core, read-only**: identity, memory, scheduler (interval and idle time),
   proactive runs over plugin snapshots and the session table, digest and
   question views, budget and kill switch, dry run, activity log.
2. **Rules, gate and review** for requested runs, with the rules editor.
3. **Delegation**: visible Windows Terminal tab first; headless only after the
   spike above.

## Open questions

- How often proactive runs happen, and on which source.
- Whether the agent may write its own notes during a proactive run (proposed:
  yes — bounded, logged, wipeable).
- Whether plugin data reaches the agent per plugin (proposed: opt-in per plugin).
