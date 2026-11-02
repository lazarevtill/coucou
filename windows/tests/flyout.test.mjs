// The tray flyout's real entry point, through the DOM and Tauri fakes. Run
// with `npm test`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { installFakeDom } from "./fakedom.mjs";
import { emit, sent } from "./tauri.mjs";

// The flyout imports its stylesheet; layout is checked in the webview.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.endsWith(".css")) {
      return { url: new URL(specifier, context.parentURL).href, shortCircuit: true };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url.endsWith(".css")) return { format: "module", source: "", shortCircuit: true };
    return nextLoad(url, context);
  },
});

const dom = installFakeDom();
const root = dom.root();
document.getElementById = () => root;
document.addEventListener = () => {};
window.addEventListener = () => {};
document.visibilityState = "visible";
const createElement = document.createElement;
document.createElement = (tag) => {
  const element = createElement(tag);
  // Mochi's drawing is not what these tests look at.
  if (tag === "canvas") element.getContext = () => null;
  return element;
};
/** Frames asked for and not run yet. */
const frames = [];
globalThis.requestAnimationFrame = (callback) => frames.push(callback);
await import("../src/flyout/main.ts");

const snapshot = (approval) => ({
  trayState: "attention", tooltip: "", headline: "", paused: false, island: "on",
  botState: "idle", approval, sessions: [], services: [],
});

test("a fresh permission card ignores clicks for a moment", (t) => {
  let now = 0;
  t.mock.method(performance, "now", () => now);
  emit("shell-snapshot", snapshot({ requestId: "A", sessionId: "s", project: "api", host: null, target: "npm test" }));
  const allow = root.find(".approval")[0].find("BUTTON")[1];
  now = 100;
  allow.fire("click");
  assert.deepEqual(sent("shell_action"), [], "a click meant for the row the card pushed down");
  now = 700;
  allow.fire("click");
  assert.deepEqual(sent("shell_action"), [{ action: { kind: "decide", requestId: "A", decision: "allow" } }]);
});
