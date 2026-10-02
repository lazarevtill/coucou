// Plugins on the front end: the switched-on map, and what the flyout shows for
// plugins from the folder. Run with `npm test`.

import test from "node:test";
import assert from "node:assert/strict";
import { isOn, withOn, builtinsOn, MAX_BUILTINS } from "../src/core/plugins.ts";
import { pluginRows } from "../src/core/shell.ts";

test("the map says what is switched on, and switching returns a new map", () => {
  const map = { integration_vercel: { enabled: true }, "status-page": { enabled: true, approvedHash: "ab" } };
  assert.equal(isOn(map, "integration_vercel"), true);
  assert.equal(isOn(map, "integration_stripe"), false);
  const off = withOn(map, "integration_vercel", false);
  assert.equal(isOn(off, "integration_vercel"), false);
  assert.equal(isOn(map, "integration_vercel"), true, "the old map is left alone");
  const on = withOn(map, "integration_stripe", true);
  assert.deepEqual(on.integration_stripe, { enabled: true });
  assert.equal(on["status-page"].approvedHash, "ab", "a plugin's approval is kept");
});

test("only built-ins count towards the pills next to Mochi", () => {
  const map = {
    integration_resend: { enabled: true },
    integration_n8n: { enabled: true },
    integration_github: { enabled: false },
    "status-page": { enabled: true, approvedHash: "ab" },
  };
  assert.deepEqual(builtinsOn(map), ["integration_n8n", "integration_resend"]);
  assert.equal(MAX_BUILTINS, 4);
});

const status = (over) => ({
  id: "status-page", name: "GitHub status", color: "#24292F", status: "ok",
  headline: "All Systems Operational", items: [], error: null, link: "https://www.githubstatus.com", ...over,
});

test("a working plugin shows its headline and opens its link", () => {
  assert.deepEqual(pluginRows({ "status-page": status({}) }), [
    { id: "status-page", name: "GitHub status", color: "#24292F", status: "ok", detail: "All Systems Operational", link: "https://www.githubstatus.com" },
  ]);
});

test("a failing, starting or changed plugin says so; one that is off is not shown", () => {
  const rows = pluginRows({
    a: status({ id: "a", status: "error", error: "HTTP 503", headline: null }),
    b: status({ id: "b", status: "waiting", headline: null }),
    c: status({ id: "c", status: "changed", error: "Changed since you approved it" }),
    d: status({ id: "d", status: "off" }),
    e: status({ id: "e", status: "invalid", error: "plugin.json: bad" }),
  });
  assert.deepEqual(rows.map((r) => [r.id, r.status, r.detail]), [
    ["a", "error", "HTTP 503"],
    ["b", "off", "Starting…"],
    ["c", "error", "Changed since you approved it"],
  ]);
});
