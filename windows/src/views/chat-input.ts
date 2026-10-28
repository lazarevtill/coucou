import { h, svg } from "./dom";
import { ICONS } from "./icons";
import { Bridge, onEvent } from "../core/bridge";
import { HOST_OS } from "../core/pills";
import { t } from "../i18n/i18n";

interface DictationEvent {
  phase: "idle" | "recording" | "transcribing";
  level: number;
  device: string;
  text: string | null;
  error: string | null;
}

/** Dictation only fills the prompt. Attaching metadata never sends a message. */
export function chatInputTools(onText: (text: string) => void, onWindow: (context: { appName: string; title: string; url?: string | null }) => void | Promise<void>, onResize: () => void) {
  let active = false;
  let phase: DictationEvent["phase"] = "idle";
  let busy = false;
  let visible = true;
  let attaching = false;
  let attachmentRequest = 0;
  const mic = h("button", { class: "chat-mic chat-tool-btn", title: t("Microphone"), "aria-label": t("Microphone") }, svg(ICONS.microphone, 12));
  const cancel = h("button", { class: "chat-mic-cancel chat-tool-btn", title: t("Cancel"), "aria-label": t("Cancel") }, svg(ICONS.xmark, 10));
  const attach = h("button", { class: "chat-window chat-tool-btn", title: t("Attach a window"), "aria-label": t("Attach a window") }, svg(ICONS.window, 12));
  const level = h("meter", { class: "chat-mic-level", min: "0", max: "1", value: "0", "aria-label": t("Microphone"), style: "width:40px" }) as HTMLMeterElement;
  const status = h("div", { class: "picker-status", role: "status" });
  const windows = h("select", { class: "chat-window-list", "aria-label": t("Attach a window") }) as HTMLSelectElement;
  const controls = h("div", { class: "chat-input-tools", style: "display:flex;align-items:center;gap:5px" }, mic, cancel, level, attach);
  const details = h("div", {}, status, windows);
  windows.hidden = true;
  function draw() {
    mic.disabled = busy || attaching || phase === "transcribing";
    mic.title = active ? t("Stop") : t("Microphone");
    mic.setAttribute("aria-label", mic.title);
    mic.setAttribute("aria-pressed", String(active));
    cancel.hidden = !active;
    level.hidden = !active;
    attach.hidden = HOST_OS !== "windows";
    attach.disabled = busy || active || attaching;
  }
  function fail(error: unknown) {
    active = false;
    phase = "idle";
    status.textContent = String(error).replace(/^Error:\s*/, "");
    draw();
    onResize();
  }
  mic.addEventListener("click", () => {
    status.textContent = "";
    if (active) {
      phase = "transcribing";
      void Bridge.chatDictationStop(false).catch(fail);
    } else {
      active = true;
      phase = "recording";
      void Bridge.chatDictationStart().catch(fail);
    }
    draw();
    onResize();
  });
  function stop() {
    if (!active) return;
    active = false;
    phase = "idle";
    status.textContent = "";
    void Bridge.chatDictationStop(true).catch(fail);
    draw();
    onResize();
  }
  cancel.addEventListener("click", stop);
  void onEvent<DictationEvent>("chat-dictation", event => {
    if (!active || !visible) return;
    phase = event.phase;
    level.value = event.level;
    level.title = event.device;
    status.textContent = event.error ?? (phase === "transcribing" ? t("Transcribing…") : phase === "recording" ? event.device : "");
    if (phase === "idle") {
      active = false;
      if (event.text) onText(event.text);
    }
    draw();
    onResize();
  });
  attach.addEventListener("click", () => {
    if (!visible || busy || active || attaching) return;
    const request = ++attachmentRequest;
    status.textContent = "";
    void Bridge.chatWindows().then(items => {
      if (!visible || request !== attachmentRequest) return;
      windows.replaceChildren(h("option", { value: "", text: t("Attach a window") }),
        ...items.map((item, i) => h("option", { value: String(i), text: `${item.appName} · ${item.title}` })));
      windows.hidden = false;
      onResize();
      windows.onchange = () => {
        const item = windows.value === "" ? null : items[Number(windows.value)];
        if (item && visible && !busy && !attaching) {
          attaching = true;
          draw();
          void Bridge.chatWindowContext(item.id).then(async context => {
            if (!visible || busy || request !== attachmentRequest) return;
            await onWindow(context);
          }).catch(error => { if (visible && request === attachmentRequest) fail(error); }).finally(() => {
            if (request === attachmentRequest) { attaching = false; draw(); onResize(); }
          });
        }
        windows.hidden = true;
        onResize();
      };
    }).catch(fail);
  });
  draw();
  return { controls, details, get active() { return active || attaching; }, sync(isVisible: boolean, sending: boolean) {
    if (visible && !isVisible) { attachmentRequest++; attaching = false; }
    visible = isVisible;
    busy = sending;
    if (!visible) { stop(); windows.hidden = true; }
    draw();
  } };
}
