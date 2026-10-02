// A tiny MCP server over stdio for Coucou's tests. No dependencies.
//
//   node mcp-test-server.mjs modern   — the current, stateless revision
//   node mcp-test-server.mjs legacy   — initialize-based; pings the client before
//                                       answering a tool call; chatty on stderr
//   node mcp-test-server.mjs strict   — legacy, and exits on anything before initialize
//
// TICKETS_KEY from the environment shows up in the tool result, so a test can
// see a secret reached the server and nothing else of the parent's environment.

import { createInterface } from "node:readline";

const mode = process.argv[2] ?? "modern";
const send = (m) => process.stdout.write(JSON.stringify(m) + "\n");
let initialized = false;
let pending = null; // a tool call waiting for the client's answer to our ping

const tools = [
  { name: "open_tickets", description: "Open tickets", inputSchema: { type: "object" }, annotations: { readOnlyHint: true } },
  { name: "close_ticket", description: "Close a ticket", inputSchema: { type: "object" } },
];

function status(args) {
  const data = {
    summary: `${3} open tickets`,
    tickets: [{ title: "Login fails", state: "open" }, { title: "Slow search", state: "open" }, { title: "Typo", state: "open" }].slice(0, args?.limit ?? 3),
    key: process.env.TICKETS_KEY ?? null,
    home: process.env.SECRET_PARENT_VALUE ?? null,
  };
  return { content: [{ type: "text", text: JSON.stringify(data) }], structuredContent: data, isError: false };
}

if (mode !== "modern") {
  // A server that logs a lot: a client that does not drain stderr blocks it.
  process.stderr.write("x".repeat(256 * 1024) + "\n");
}

const rl = createInterface({ input: process.stdin });
rl.on("line", (line) => {
  let m;
  try { m = JSON.parse(line); } catch { return; }

  // The client's reply to our ping.
  if (m.id === "ping-1" && "result" in m && pending) {
    send({ jsonrpc: "2.0", id: pending.id, result: status(pending.params.arguments) });
    pending = null;
    return;
  }
  if (!m.method) return;

  if (mode === "modern") {
    const meta = m.params?._meta ?? {};
    if (meta["io.modelcontextprotocol/protocolVersion"] !== "2026-07-28") {
      return send({ jsonrpc: "2.0", id: m.id, error: { code: -32022, message: "Unsupported protocol version", data: { supported: ["2026-07-28"], requested: meta["io.modelcontextprotocol/protocolVersion"] ?? null } } });
    }
    if (m.method === "server/discover") return send({ jsonrpc: "2.0", id: m.id, result: { resultType: "complete", supportedVersions: ["2026-07-28"], capabilities: { tools: {} } } });
    if (m.method === "tools/list") return send({ jsonrpc: "2.0", id: m.id, result: { resultType: "complete", tools } });
    if (m.method === "tools/call") return send({ jsonrpc: "2.0", id: m.id, result: { resultType: "complete", ...status(m.params.arguments) } });
    return send({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: "Method not found" } });
  }

  // Legacy (2025-11-25).
  if (m.method === "initialize") {
    initialized = true;
    return send({ jsonrpc: "2.0", id: m.id, result: { protocolVersion: "2025-11-25", capabilities: { tools: {} }, serverInfo: { name: "test", version: "1" } } });
  }
  if (m.method === "notifications/initialized") return;
  if (!initialized) {
    if (mode === "strict") process.exit(3);
    return send({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: "Method not found" } });
  }
  if (m.method === "tools/list") return send({ jsonrpc: "2.0", id: m.id, result: { tools } });
  if (m.method === "tools/call") {
    pending = m;
    return send({ jsonrpc: "2.0", id: "ping-1", method: "ping" });
  }
  send({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: "Method not found" } });
});
rl.on("close", () => process.exit(0));
