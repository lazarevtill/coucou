import { h, clear } from "../views/dom";
import { t } from "../i18n/i18n";
import type { AudioSourceStatus } from "../core/flow";

/** Refresh endpoint choices after hotplug without replacing the focused select. */
export function updateAudioDevices(select: HTMLSelectElement, devices: { id: string; name: string; default: boolean }[], selected: string, windows: boolean) {
  const automatic = windows ? t("Automatic — follow call app") : t("System default");
  const choices: [string, string][] = [["", automatic], ...devices.map((d): [string, string] => [d.id, d.name])];
  if (selected && !devices.some(d => d.id === selected)) choices.push([selected, t("Selected device (disconnected)")]);
  const signature = JSON.stringify(choices);
  if (select.dataset.devices !== signature) {
    clear(select);
    for (const [value, text] of choices) select.append(h("option", { value, text }));
    select.dataset.devices = signature;
  }
  select.value = selected;
}

export function levelDb(level: number): number {
  return Number.isFinite(level) && level > 0 ? Math.max(-60, Math.min(0, 20 * Math.log10(level))) : -60;
}

/** Shared by Notetaker's live sources and the short Settings audio check. */
export function audioMeters() {
  const element = h("div", { class: "hub-audio-meters", style: "display:grid;gap:12px;margin:12px 0" });
  return { element, update(sources: AudioSourceStatus[]) {
    clear(element);
    for (const source of sources) {
      const name = source.source === "microphone" ? t("Microphone") : t("Call playback");
      const db = levelDb(source.level);
      const health = source.error || (!source.connected ? t("Unavailable")
        : source.clipping ? t("Clipping — lower the source volume")
        : source.peak > 0.003 ? t("Sound detected") : t("Waiting for sound"));
      const meter = h("div", { class: "hub-progress", role: "meter", "aria-label": name,
        "aria-valuemin": "-60", "aria-valuemax": "0", "aria-valuenow": String(Math.round(db)), style: "width:100%;height:7px" },
        h("i", { style: `width:${(db + 60) / 60 * 100}%;background:${source.clipping ? "#F4505E" : "#34D399"}` }));
      element.append(h("div", {},
        h("div", { class: "hub-row", style: "padding:0 0 5px" }, h("b", { text: name }),
          h("span", { class: "hub-note", text: `${Math.round(db)} dBFS` })),
        h("div", { class: "hub-note", text: source.device || t("No device"), style: "margin-bottom:5px" }),
        source.application ? h("div", { class: "hub-note", text: t("Following {0}", { 0: source.application }) }) : null,
        meter,
        h("div", { class: source.error || !source.connected || source.clipping ? "hub-warn" : "hub-note", text: health }),
        ...[...new Set(source.warnings ?? [])].filter((warning) => warning !== source.error).map((text) => h("div", { class: "hub-warn", text })),
        source.sampleRate ? h("div", { class: "hub-note", text: `${source.sampleRate.toLocaleString()} Hz · ${t("{0} channels", { 0: source.channels })}` }) : null,
      ));
    }
  } };
}
