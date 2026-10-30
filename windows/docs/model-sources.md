# Chat sources

The chat model picker offers Claude Code, Codex, Anthropic, Google, OpenAI, OpenRouter, opted-in Cursor/Copilot subscriptions, and configured model servers. It shows the selected provider/model above the message field. Provider changes retain the plain-text conversation; provider-specific tool blocks are not forwarded between services.

## Existing agent sign-ins

**Settings → Agents → Use for chat** selects Claude Code or Codex without an API key. Both also offer **Open project** for native coding sessions. See [coding-agents.md](coding-agents.md) for setup, permissions, limits and verification.

**Settings → Chat → Your AI subscriptions** routes a provider through its installed official terminal tool after an explicit opt-in. Anthropic uses Claude Code; OpenAI uses Codex. Both share the same supervised runner and image/PDF support as the explicit connections, with tools disabled. The CLI chooses its default model; the saved API model remains unchanged. **Use for all text** on either explicit connection also configures Flow text tasks.

Google uses Antigravity CLI or Gemini CLI; Cursor and Copilot use their respective CLIs. These upstream relays use plan/ask or restricted tool modes in a private working folder. They may inherit the CLI's configuration; these modes are not an OS sandbox. They support inline text attachments, with a size limit, and remain opt-in. All relays have bounded input/output, a deadline and Stop control. Installing a CLI does not change an API provider to subscription billing.

## API providers

Add the relevant key in **Settings → Chat**, then choose the provider and model in the chat picker. API keys use Windows Credential Manager or Linux Secret Service. They are separate from Claude/ChatGPT subscription sign-ins. Models are listed by the selected provider; file support depends on that provider.

## Model servers

**Settings → Chat → Local models** connects Ollama, LM Studio or an OpenAI-compatible server. Enter its address, connect, and select one of the models it reports. A server is visible in the chat picker once configured, or while selected.

An optional custom-server key is bound to the exact normalized address it was entered for. Changing an address cannot send that key to the new server. Keys are sent only over HTTPS or to loopback. The connection view indicates whether an address is on this computer or elsewhere. Redirects are refused.

Local-model replies stream into the chat. Reasoning blocks are separated from the visible answer. Text attachment support and supported image formats depend on the provider; unsupported files produce an error.

## Settings and credentials

Windows editor, terminal, island and notification preferences are retained. Folder plugins keep their approved manifest hashes; built-in service selections are stored separately. Model-server address and model preferences from a legacy settings file are imported into the custom-server fields without replacing explicit newer values. The first save keeps a dated `settings.json.migrated-*` copy.

A legacy server credential can be imported into the address-bound key entry using the original saved address; an existing bound key is never overwritten. The old credential remains in the OS store. If the credential store is inaccessible or the old address is remote plain HTTP, enter a key again beside an HTTPS address in Settings. No CLI credential is migrated.

Tests in `local_chat.rs`, `net.rs`, `chat.rs`, `cli_agents.rs`, `settings.rs` and `tests/chat.test.mjs` cover the relevant protocol, address, migration and conversation contracts.
