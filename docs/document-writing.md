# Writing docs

How to add and change the Markdown docs in this repository (every `*.md` file).

## Where docs live
- `docs/`: upstream's product specs and protocols (`SPEC.md`, `INTEGRATIONS.md` and `VOICE.md` are in French), plus this guide.
- `windows/docs/`: Windows and Linux. `android/`: Android. Each relay has its own README (`relay/`, `android/relay/`).
- Run `git ls-files '*.md'` and look for an existing doc before adding one.

## Shape
- One topic per file, with a kebab-case file name that says the topic.
- Start each doc with `# Title` and one line naming what it covers and its code paths.
- Headings form the outline that agents read first.
- README stays slim; depth goes to docs.

## Rules and content
- [AGENTS.md](../AGENTS.md) holds the fork's rules; `## Rules everywhere` applies to every area. Area rules currently live in AGENTS.md under `## Windows and Linux rules`; when an area's rules grow, they may move to that area's doc under `## Rules for changes`, one rule per bullet, stated as current behavior to keep.
- [CLAUDE.md](../CLAUDE.md) is upstream's; do not edit it in the fork.
- Describe current state only: no history, no "previously" or "fixed" narrative; git has the history. Delete superseded text.
- Update docs in the same change as the code.
- No secrets, no personal infrastructure.
