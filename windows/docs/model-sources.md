# Chat sources

The chat answers with Claude (the Anthropic API) or with your own model server —
anything that speaks the OpenAI chat completions API: llama.cpp's
`llama-server`, LM Studio, Ollama, vLLM.

## Choosing one

**Settings… → Chat → Answers from**:

- **Claude (Anthropic API)** — the key goes in **Settings… → Claude**. Web
  search, PDFs and images work here.
- **Your model server** — fill in:
  - **Address**: the server's base URL up to and including `/v1`.
    `llama-server` with no options listens on `http://127.0.0.1:8080/v1`.
    **Check** asks the server for its models and only keeps an address that
    answers.
  - **Model**: one the server lists, or *First listed*.
  - **API key**: only if the server asks for one (`llama-server --api-key …`).
    It is stored in the Windows Credential Manager like every other key.

The chat field says who answers — *Ask Claude…*, *Ask your model server…* or the
model's name. Switching source starts a new conversation.

## What is sent where

- With your model server, the conversation goes to that address and nowhere else.
- Plain `http://` is accepted only for this computer and the local network:
  `localhost`, `127.0.0.0/8`, `::1`, `10.0.0.0/8`, `172.16.0.0/12`,
  `192.168.0.0/16`, link-local addresses, and names ending in `.local`, `.lan`,
  `.home.arpa`, `.internal`. Anything else needs `https://`.
- Addresses with a user name or password, a `?query` or a `#fragment` are
  refused. Redirects are not followed, so a local address can never send the
  conversation somewhere else.
- A model on the local network may take up to ten minutes over an answer (a
  model running on the processor is slow); a remote one two minutes.

## What works with your own server

- Multi-turn conversation, as with Claude.
- Text and code files dropped on the island, inlined up to 200 KB.
- Not PDFs or images: the chat says so before anything is sent. Switch to Claude
  for those.
- No web search.
- Reasoning models (gpt-oss and the like) keep their thinking apart from their
  answer; only the answer is shown. If a model thinks without ever answering,
  the chat says it may need more tokens.

## How this is tested

Automated (`cargo test --workspace`): the address rules, the request, reading
answers (text, text in parts, reasoning without an answer, server errors), the
model list in both the OpenAI and the Ollama shape, timeouts, and whole
conversations against an HTTP server started inside the test — the history,
the key as a Bearer token, a failed turn not resent, redirects refused, a PDF
refused before anything is sent.

End to end, on a separate instance (see
[terminals.md](terminals.md#how-this-is-tested)) and a real `llama-server`
(build 11193) running SmolLM2-135M-Instruct (Q4_K_M, about 105 MB) on
`127.0.0.1:18080`:

- **Check** in the settings window found the server and its model;
- the island's chat answered, and a follow-up question kept the conversation;
- a text file's content was used; a PDF was refused;
- a stopped server gave *Can't reach the model server at …*;
- with `--api-key`, no key gave the server's own *401: Invalid API Key*, and the
  key saved in the settings window made it work;
- an `http://` address on the internet was refused in the settings window;
- switching back to Claude sent the next question to the Anthropic client.
