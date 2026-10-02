// Run with `npm test`. The payload shapes are Claude Code 2.1.287's, captured from
// a real run on a sample file and passed through the relay (which keeps only the
// patch of a tool's result).

import test from "node:test";
import assert from "node:assert/strict";
import { diffLines, editFromPost, editFromPre, fileBadge, shownPath, SHOWN_LINES } from "../src/core/edits.ts";

const FILE = "C:\\work\\shop\\src\\invoice.ts";
const lines = (e) => e.lines.map((l) => `${l.num ?? "·"} ${{ ctx: " ", del: "-", add: "+", gap: "…" }[l.kind]} ${l.text}`.trimEnd());

test("a file's badge comes from its extension", () => {
  assert.equal(fileBadge(FILE), "TS");
  assert.equal(fileBadge("C:\\x\\main.rs"), "RS");
  assert.equal(fileBadge("C:\\x\\README"), "TXT");
  assert.equal(fileBadge("C:\\x\\.env"), "ENV");
  assert.equal(fileBadge("C:\\x\\notes.markdown"), "MARK", "four letters at most");
});

test("a line diff keeps what did not change and puts removals before additions", () => {
  const d = diffLines(["a", "b", "c"], ["a", "B", "c", "d"]);
  assert.deepEqual(d.map((l) => `${l.kind}:${l.text}`), ["ctx:a", "del:b", "add:B", "ctx:c", "add:d"]);
  assert.ok(d.every((l) => l.num === null), "without the file, no line numbers are made up");
  assert.deepEqual(diffLines([], ["x"]).map((l) => l.kind), ["add"]);
});

test("an Edit about to run shows its old and new text, unnumbered", () => {
  const e = editFromPre("Edit", { file_path: FILE, old_string: "const TVA = 0.196", new_string: "const TVA = 0.2", replace_all: false });
  assert.equal(e.name, "invoice.ts");
  assert.equal(e.badge, "TS");
  assert.equal(e.status, "pending");
  assert.equal(e.numbered, false);
  assert.deepEqual(lines(e), ["· - const TVA = 0.196", "· + const TVA = 0.2"]);
  assert.equal(e.added, 1);
  assert.equal(e.removed, 1);
  assert.equal(e.cut, false);
});

test("the applied Edit shows Claude Code's own hunks with the file's line numbers", () => {
  const patch = {
    structuredPatch: [{
      oldStart: 1, oldLines: 7, newStart: 1, newLines: 7,
      lines: [" // A sample file.", " import { Item } from './types'", " ", "-const TVA = 0.196", "+const TVA = 0.2", " ", " export function total(items: Item[]) {"],
    }],
  };
  const e = editFromPost("Edit", { file_path: FILE, old_string: "const TVA = 0.196", new_string: "const TVA = 0.2" }, patch);
  assert.equal(e.status, "applied");
  assert.equal(e.numbered, true);
  assert.deepEqual(lines(e), [
    "1   // A sample file.",
    "2   import { Item } from './types'",
    "3",
    "4 - const TVA = 0.196",
    "4 + const TVA = 0.2",
    "5",
    "6   export function total(items: Item[]) {",
  ]);
  assert.equal(e.added, 1);
  assert.equal(e.removed, 1);
});

test("hunks far apart are separated, and a replace_all edit says so", () => {
  const patch = {
    structuredPatch: [
      { oldStart: 7, oldLines: 1, newStart: 7, newLines: 1, lines: ["-  const sum = 1", "+  const subtotal = 1"] },
      { oldStart: 12, oldLines: 1, newStart: 12, newLines: 1, lines: ["-  return sum", "+  return subtotal"] },
    ],
  };
  const e = editFromPost("Edit", { file_path: FILE, old_string: "sum", new_string: "subtotal", replace_all: true }, patch);
  assert.deepEqual(lines(e), ["7 -   const sum = 1", "7 +   const subtotal = 1", "· …", "12 -   return sum", "12 +   return subtotal"]);
  assert.equal(e.replaceAll, true);
  assert.equal(e.added, 2);
});

test("a file Write created is all new lines, numbered from one", () => {
  const e = editFromPost("Write", { file_path: "C:\\work\\shop\\notes.md", content: "alpha\nbeta" }, { type: "create", structuredPatch: [] });
  assert.equal(e.created, true);
  assert.equal(e.numbered, true);
  assert.deepEqual(lines(e), ["1 + alpha", "2 + beta"]);
});

test("a Write over a file shows what changed in it", () => {
  const e = editFromPost("Write", { file_path: "C:\\work\\shop\\notes.md", content: "gamma" }, {
    type: "update", structuredPatch: [{ oldStart: 1, oldLines: 2, newStart: 1, newLines: 1, lines: ["-alpha", "-beta", "+gamma"] }],
  });
  assert.equal(e.created, false);
  assert.deepEqual(lines(e), ["1 - alpha", "2 - beta", "1 + gamma"]);
});

test("without a patch (an older relay) there is nothing better than the text before", () => {
  assert.equal(editFromPost("Edit", { file_path: FILE, old_string: "a", new_string: "b" }, undefined), null);
  assert.equal(editFromPost("Edit", { file_path: FILE }, { structuredPatch: "nope" }), null);
});

test("only Edit and Write are edits", () => {
  assert.equal(editFromPre("Read", { file_path: FILE }), null);
  assert.equal(editFromPre("Bash", { command: "rm -rf x" }), null);
  assert.equal(editFromPre("Edit", { old_string: "a", new_string: "b" }), null, "no file, no edit");
  assert.equal(editFromPost("NotebookEdit", { notebook_path: FILE }, { structuredPatch: [] }), null);
});

test("text the relay cut is never shown as if it were whole", () => {
  // The relay cuts a string at 2000 bytes and appends an ellipsis.
  const long = "x".repeat(2000) + "…";
  const e = editFromPre("Edit", { file_path: FILE, old_string: "short", new_string: long });
  assert.equal(e.cut, true);
  const written = editFromPre("Write", { file_path: FILE, content: long });
  assert.equal(written.cut, true);
  const capped = editFromPost("Edit", { file_path: FILE }, { structuredPatch: [{ oldStart: 1, oldLines: 0, newStart: 1, newLines: 1, lines: ["+a"] }], truncated: true });
  assert.equal(capped.cut, true);
  assert.equal(editFromPre("Edit", { file_path: FILE, old_string: "a", new_string: "ends with …" }).cut, false, "a short text ending in … is whole");
});

test("a big edit keeps a bounded number of lines and says it is cut", () => {
  const content = Array.from({ length: 300 }, (_, i) => `line ${i}`).join("\n");
  const e = editFromPost("Write", { file_path: FILE, content }, { type: "create", structuredPatch: [] });
  assert.equal(e.lines.length, SHOWN_LINES);
  assert.equal(e.cut, true);
  assert.equal(e.added, 300, "the count is of the whole edit");
});

test("a patch's odd lines are skipped, not shown as code", () => {
  const e = editFromPost("Edit", { file_path: FILE }, {
    structuredPatch: [{ oldStart: 3, oldLines: 1, newStart: 3, newLines: 1, lines: ["-a", "+b", "\\ No newline at end of file", 42] }],
  });
  assert.deepEqual(lines(e), ["3 - a", "3 + b"]);
});

test("a file inside the session's folder is shown by its path in it", () => {
  assert.equal(shownPath(String.raw`C:\work\shop\src\invoice.ts`, String.raw`C:\work\shop`), String.raw`src\invoice.ts`);
  assert.equal(shownPath(String.raw`c:\Work\Shop\src\invoice.ts`, String.raw`C:\work\shop\ `.trim()), String.raw`src\invoice.ts`, "Windows paths ignore case");
  assert.equal(shownPath(String.raw`C:\work\shopfront\a.ts`, String.raw`C:\work\shop`), String.raw`C:\work\shopfront\a.ts`, "a neighbour is not inside");
  assert.equal(shownPath(String.raw`D:\other\a.ts`, ""), String.raw`D:\other\a.ts`);
  assert.equal(shownPath("C:/work/shop/src/a.ts", String.raw`C:\work\shop`), "src/a.ts", "Claude Code may use forward slashes");
});
