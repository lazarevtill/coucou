import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { calls, internals, sent } from "./tauri.mjs";

installFakeDom();
const { buildChoose, buildMail } = await import("../src/views/upload.ts");
const { State } = await import("../src/core/state.ts");
const invoke = internals.invoke;
let shareResult;
internals.invoke = async (cmd, args) => {
  const result = await invoke(cmd, args);
  if (cmd === "file_share_available") return true;
  if (cmd === "share_file") return shareResult?.();
  return result;
};
const flush = () => new Promise(resolve => setTimeout(resolve, 0));
beforeEach(() => { calls.length = 0; shareResult = null; State.droppedFile = { name: "<draft>.txt", path: "C:/inbox/draft.txt", size: 5 }; });

test("email is offered as an explicit action for an already dropped file", async () => {
  let selected;
  const view = buildChoose({ setView: value => { selected = value; }, cancelDrop() {} });
  await flush();
  view.sync();
  const action = view.el.find("button").find(button => button.textContent === "Send by email");
  assert.equal(action.hidden, false);
  action.fire("click");
  assert.equal(selected, "mail");
  assert.equal(sent("share_file").length, 0);
});

test("sharing passes only the chosen inbox file after a click and prevents duplicate requests", async () => {
  let finish;
  shareResult = () => new Promise(resolve => { finish = resolve; });
  const view = buildMail({ setView() {} });
  view.sync();
  assert.match(view.el.textContent, /<draft>\.txt/);
  assert.equal(calls.length, 0);
  const button = view.el.querySelector(".primary");
  button.fire("click");
  await flush();
  view.sync();
  button.fire("click");
  assert.equal(sent("share_file").length, 1);
  assert.equal(sent("share_file")[0].path, "C:/inbox/draft.txt");
  finish();
  await flush();
  assert.equal(button.disabled, false);
});

test("native sharing errors are visible and an absent file cannot be shared", async () => {
  shareResult = () => { throw new Error("No mail app available"); };
  const view = buildMail({ setView() {} });
  view.sync();
  view.el.querySelector(".primary").fire("click");
  await flush();
  assert.match(view.el.textContent, /No mail app available/);
  State.droppedFile = null;
  view.sync();
  assert.equal(view.el.querySelector(".primary").disabled, true);
  assert.doesNotMatch(view.el.textContent, /No mail app available/);
});
