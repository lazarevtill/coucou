import { afterAll, describe, expect, it, vi } from "vitest";
import { WebSocket } from "ws";
import { startRelay } from "../tools/dev-relay.ts";
import { ACCESS, behaviour, offer, open, proofOf, randomRoom, type Conn, type Harness, type Msg, type Options } from "./behaviour.ts";

let relay: Awaited<ReturnType<typeof startRelay>>;

function wrap(ws: WebSocket): Conn {
  const queue: Msg[] = [];
  let wake: (() => void) | undefined;
  const push = (m: Msg) => {
    queue.push(m);
    wake?.();
  };
  ws.on("message", (data, isBinary) => {
    const buf = Buffer.isBuffer(data) ? data : Buffer.concat(data as Buffer[]);
    push(isBinary ? { kind: "binary", data: new Uint8Array(buf) } : { kind: "text", data: buf.toString() });
  });
  ws.on("close", (code, reason) => push({ kind: "close", code, reason: reason.toString() }));
  const next = async (timeoutMs = 1500): Promise<Msg> => {
    const until = Date.now() + timeoutMs;
    while (queue.length === 0) {
      if (Date.now() > until) throw new Error("timed out waiting for a message");
      await new Promise<void>((r) => {
        wake = r;
        setTimeout(r, 25);
      });
    }
    return queue.shift()!;
  };
  return {
    next,
    quiet: async (ms = 200) => {
      const until = Date.now() + ms;
      while (Date.now() < until) {
        if (queue.length > 0) return false;
        await new Promise((r) => setTimeout(r, 15));
      }
      return queue.length === 0;
    },
    send: (d) => ws.send(d),
    close: (code, reason) => ws.close(code ?? 1000, reason ?? ""),
  };
}

const harness = async (): Promise<Harness> => {
  relay = await startRelay({ accessKey: ACCESS });
  const base = `127.0.0.1:${relay.port}`;
  return {
    async http(path, method = "GET") {
      const r = await fetch(`http://${base}${path}`, { method });
      return { status: r.status, text: await r.text() };
    },
    connect(room, role, o: Options = {}) {
      const protocols = o.protocols ?? offer(o.proof ?? proofOf(1));
      const headers: Record<string, string> = {};
      if (o.key !== null) headers.Authorization = `Bearer ${o.key ?? ACCESS}`;
      return new Promise((resolve) => {
        const ws = new WebSocket(`ws://${base}/v1/room/${room}?role=${role}`, protocols ? protocols.split(",").map((s) => s.trim()).filter(Boolean) : [], { headers });
        ws.on("unexpected-response", (_req, res) => {
          resolve({ refused: res.statusCode ?? 0 });
          ws.terminate();
        });
        ws.on("error", () => {});
        ws.on("open", () => resolve(wrap(ws)));
      });
    },
    setAccessKey: async (key) => relay.setAccessKey(key),
    roomsCreated: async () => relay.rooms(),
    stop: () => relay.close(),
  };
};

behaviour("Node twin", harness);

describe("Node twin only", () => {
  it("preserves frame order when cryptographic checks complete at different speeds", async () => {
    const h = await harness();
    const room = randomRoom();
    const pc = await open(h, room, "pc");
    const phone = await open(h, room, "phone");
    await pc.next();
    await phone.next();
    const digest = crypto.subtle.digest.bind(crypto.subtle);
    const spy = vi.spyOn(crypto.subtle, "digest");
    let checks = 0;
    spy.mockImplementation(async (...args) => {
      const first = checks++ === 0;
      const hash = await digest(...args);
      if (first) await new Promise((resolve) => setTimeout(resolve, 100));
      return hash;
    });
    try {
      for (const [sender, receiver] of [[phone, pc], [pc, phone]]) {
        checks = 0;
        for (let i = 0; i < 20; i++) sender.send(new Uint8Array([i]));
        for (let i = 0; i < 20; i++) {
          expect(await receiver.next()).toEqual({ kind: "binary", data: new Uint8Array([i]) });
        }
      }
    } finally {
      spy.mockRestore();
      await h.stop();
    }
  });

  it("answers 429 after too many new connections from one address, before checking the key", async () => {
    const r = await startRelay({ accessKey: ACCESS, connectsPerMinute: 3 });
    try {
      const statuses: number[] = [];
      for (let i = 0; i < 5; i++) {
        statuses.push(
          await new Promise<number>((resolve) => {
            const ws = new WebSocket(`ws://127.0.0.1:${r.port}/v1/room/${randomRoom()}?role=pc`, ["coucou.v1", `coucou.join.${proofOf(1)}`], { headers: { Authorization: "Bearer wrong-wrong-wrong-wrong" } });
            ws.on("unexpected-response", (_q, res) => (resolve(res.statusCode ?? 0), ws.terminate()));
            ws.on("error", () => {});
          }),
        );
      }
      expect(statuses).toEqual([401, 401, 401, 429, 429]);
    } finally {
      await r.close();
    }
  });

  it("leaves no room behind when everyone has left", async () => {
    const r = await startRelay({ accessKey: ACCESS });
    try {
      const ws = new WebSocket(`ws://127.0.0.1:${r.port}/v1/room/${randomRoom()}?role=pc`, ["coucou.v1", `coucou.join.${proofOf(1)}`], { headers: { Authorization: `Bearer ${ACCESS}` } });
      await new Promise((res) => ws.on("open", res));
      expect(r.rooms()).toBe(1);
      ws.close();
      await new Promise((res) => ws.on("close", res));
      await new Promise((res) => setTimeout(res, 50));
      expect(r.rooms()).toBe(0);
    } finally {
      await r.close();
    }
  });
});
afterAll(() => undefined);
