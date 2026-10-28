import { h, svg } from "./dom";
import { ICONS } from "./icons";
import { Bridge } from "../core/bridge";
import { HOST_OS } from "../core/pills";
import { t } from "../i18n/i18n";
import LIMITS from "../chat-image-limits.json";

/** Only a user paste event supplies pixels. No clipboard reads or polling. */
export function chatImages(input: HTMLInputElement, onAttach: (file: { name: string; path: string }) => Promise<void>, onRemove: () => void, changed: () => void) {
  let visible = true;
  let blocked = false;
  let pending = false;
  let generation = 0;
  let attachedPath: string | null = null;
  const status = h("div", { class: "chat-image-status picker-status", role: "status" });
  const picture = h("img", { class: "chat-image-preview", alt: t("Screenshot"), style: "max-width:100%;max-height:140px;object-fit:contain" }) as HTMLImageElement;
  const remove = h("button", { class: "chat-image-remove btn secondary", text: t("Remove attachment"), onclick: () => {
    if (blocked || pending || remove.disabled) return;
    attachedPath = null;
    picture.removeAttribute("src");
    preview.hidden = true;
    onRemove();
    changed();
  } });
  const preview = h("div", { class: "chat-image" }, h("div", { style: "display:flex;align-items:center;gap:8px" }, picture, remove));
  preview.hidden = true;
  const capture = h("button", { class: "chat-capture chat-tool-btn", title: t("Screenshot"), "aria-label": t("Screenshot") }, svg(ICONS.camera, 12));
  capture.hidden = HOST_OS !== "windows";
  capture.addEventListener("click", () => {
    if (!visible || blocked || pending) return;
    const ticket = generation;
    status.textContent = t("Capture and annotate in Snipping Tool, copy the image, then paste it here.");
    void Bridge.chatCaptureOpen().catch(error => {
      if (visible && ticket === generation) { status.textContent = String(error).replace(/^Error:\s*/, ""); changed(); }
    });
    changed();
  });
  input.addEventListener("paste", event => {
    const files = Array.from((event as ClipboardEvent).clipboardData?.files ?? []);
    if (!files.some(file => file.type.startsWith("image/"))) return;
    event.preventDefault();
    if (!visible || blocked || pending) return;
    if (files.length !== 1 || files[0].type !== "image/png") {
      status.textContent = t("Paste a PNG image into the chat."); changed(); return;
    }
    const file = files[0];
    if (file.size > LIMITS.maxBytes) {
      status.textContent = t("The image is too large. Capture a smaller area."); changed(); return;
    }
    const ticket = ++generation;
    pending = true;
    capture.disabled = true;
    status.textContent = "";
    changed();
    void (async () => {
      const bytes = new Uint8Array(await file.arrayBuffer());
      if (!visible || ticket !== generation) return;
      let binary = "";
      for (let i = 0; i < bytes.length; i += 8192) binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
      const data = btoa(binary);
      const saved = await Bridge.chatImageAttach(data);
      if (!visible || ticket !== generation) return;
      await onAttach(saved);
      if (!visible || ticket !== generation) return;
      attachedPath = saved.path;
      picture.src = `data:image/png;base64,${data}`;
      preview.hidden = false;
    })().catch(error => {
      if (visible && ticket === generation) status.textContent = String(error).replace(/^Error:\s*/, "");
    }).finally(() => {
      if (ticket === generation) { pending = false; capture.disabled = blocked; changed(); }
    });
  });
  return { control: capture, preview, status, get active() { return pending; },
    sync(show: boolean, busy: boolean, currentPath: string | null, hasMessages: boolean) {
      if (visible && !show) { generation++; pending = false; status.textContent = ""; }
      visible = show;
      blocked = busy;
      capture.disabled = busy || pending;
      remove.disabled = busy || pending || hasMessages;
      if (attachedPath && attachedPath !== currentPath) {
        attachedPath = null; picture.removeAttribute("src"); preview.hidden = true;
      }
    },
  };
}
