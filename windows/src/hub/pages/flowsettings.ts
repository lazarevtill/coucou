// Flow → Settings: the key, where the pill is, the microphone, the voice
// model, the Notetaker's summaries, the window and the history. Port of
// FlowHubSettings.swift.

import { h, clear } from "../../views/dom";
import { t } from "../../i18n/i18n";
import { onEvent } from "../../core/bridge";
import { FlowBridge, type Edge, type FlowKey, type ModelsStatus, type FlowPrefs, type AudioSourceStatus } from "../../core/flow";
import { audioMeters, updateAudioDevices } from "../audio";
import type { HubContext } from "../shell";
import { button, card, divider, keyKeycap, keycap, label, number, row, segmented, select, toggle } from "../ui";

export function textEngines(): [FlowPrefs["summaryEngine"], string][] {
  return [["claude-code", "Claude Code"], ["codex", "Codex"], ["claude", t("Claude (your key)")],
    ["ollama", "Ollama"], ["lmstudio", "LM Studio"], ["custom", t("OpenAI-compatible server")], ["off", t("No summary")]];
}

/** Download / progress / installed, for the voice model. Redraws itself. */
export function modelsControl(ctx: HubContext, onReady?: () => void): HTMLElement {
  const el = h("div", { style: "display:flex;align-items:center;gap:8px" });
  let confirming = false;
  const draw = (models: ModelsStatus) => {
    clear(el);
    const m = models.parakeet;
    if (m.state === "missing") {
      el.append(button(t("Download"), () => void FlowBridge.download(), "primary"));
    } else if (m.state === "downloading") {
      const bar = h("div", { class: "hub-progress" }, h("i", { style: `width:${Math.round(m.fraction * 100)}%` }));
      el.append(bar, h("span", { class: "hub-note", text: `${Math.round(m.fraction * 100)} %` }));
    } else if (m.state === "failed") {
      el.append(h("span", { class: "hub-note", text: m.error, title: m.error, style: "max-width:220px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" }));
      el.append(button(t("Retry"), () => void FlowBridge.download()));
    } else if (confirming) {
      el.append(
        button(t("Remove the models"), () => {
          confirming = false;
          void FlowBridge.removeModels().catch(error => {
            draw(ctx.status.models);
            el.append(h("span", { class: "hub-warn", role: "alert", text: String(error) }));
          });
        }, "danger", true),
        button(t("Cancel"), () => {
          confirming = false;
          draw(ctx.status.models);
        }, "", true),
      );
    } else {
      const ok = h("button", { class: "hub-link", style: "color:var(--hub-green)", text: `✓ ${t("Installed")}`, title: t("Remove the models") });
      ok.addEventListener("click", () => {
        confirming = true;
        draw(ctx.status.models);
      });
      el.append(ok);
    }
  };
  draw(ctx.status.models);
  void onEvent<ModelsStatus>("speech-models", (models) => {
    if (!el.isConnected) return;
    const wasReady = ctx.status.models.parakeet.state === "ready";
    ctx.status.models = models;
    draw(models);
    if (!wasReady && models.parakeet.state === "ready") onReady?.();
  });
  return el;
}

export async function flowSettingsPage(page: HTMLElement, ctx: HubContext) {
  const status = await ctx.refresh();
  const prefs = status.prefs;
  page.append(ctx.hero("flowsettings", t("Settings"), null));

  // ── Keys ──
  const keys: [FlowKey, string][] = [
    ["right_ctrl", t("Right {0}", { 0: "Ctrl" })],
    ["right_alt", t("Right {0}", { 0: "Alt" })],
    ["right_shift", t("Right {0}", { 0: "Shift" })],
    ["left_ctrl_super", status.platform === "windows" ? "Ctrl + Win" : "Ctrl + Super"],
  ];
  const edges: [Edge, string][] = [["bottom", t("Bottom")], ["left", t("Left")], ["right", t("Right")]];
  page.append(label(t("Keys")));
  const keysCard = card(
    row(t("Hold a key to dictate anywhere"), null, toggle(prefs.enabled, (on) => void ctx.setPrefs((p) => (p.enabled = on)))),
    divider(),
    status.keys === "wayland"
      ? row(t("Hold to dictate"), null, keyKeycap("wayland", status.platform))
      : row(t("Hold to dictate"), prefs.key === "right_alt" ? t("On a keyboard with AltGr, the right Alt types characters: pick another key.") : null,
        select(keys, prefs.key, (key) => void ctx.setPrefs((p) => (p.key = key)).then(() => ctx.navigate("flowsettings")))),
    divider(),
    row(t("Where the pill is"), null, segmented(edges, prefs.edge, (edge) => void ctx.setPrefs((p) => (p.edge = edge)))),
    divider(),
    row(t("Hands-free"), t("Tap twice, speak as long as you like, tap once to paste."), keyKeycap(status.keys === "wayland" ? "wayland" : prefs.key, status.platform, true)),
    divider(),
    row(t("Paste the last dictation again"), t("When the words went to the wrong place."), button(t("Paste again"), () => void FlowBridge.pasteAgain(), "", true)),
    divider(),
    row(t("Cancel a dictation"), null, keycap(t("Esc"))),
  );
  if (status.keys === "wayland") {
    keysCard.append(h("div", {
      class: "hub-warn",
      text: t("On Wayland, the desktop holds the dictation key for Coucou: Ctrl+Alt+V, or the keys you picked when it asked. The first paste asks once to let Coucou type for you."),
    }));
  } else if (status.keys !== "ok") {
    keysCard.append(h("div", { class: "hub-warn", text: t("Coucou can't watch the keyboard here: use the pill's microphone to dictate.") }));
  }
  page.append(keysCard);

  // ── Microphone ──
  page.append(label(t("Microphone")));
  const micSelect = select([], prefs.mic, (mic) => void ctx.setPrefs((p) => (p.mic = mic))) as HTMLSelectElement;
  const playbackSelect = select([], prefs.speakers, (device) => void ctx.setPrefs(p => (p.speakers = device))) as HTMLSelectElement;
  updateAudioDevices(micSelect, status.mics, prefs.mic, status.platform === "windows");
  updateAudioDevices(playbackSelect, status.speakers, prefs.speakers, status.platform === "windows");
  page.append(card(row(t("Microphone"), t("The one Coucou listens to for dictation and calls."), micSelect)));
  const meters = audioMeters();
  let testing = false;
  let visible = (await FlowBridge.hubShown()) !== false;
  const testMessage = h("div", { class: "hub-note" });
  const testButton = button(t("Test audio"), () => {
    if (testing) { void FlowBridge.stopAudioTest(); return; }
    testing = true;
    testButton.textContent = t("Stop");
    testMessage.textContent = t("Speak and play call audio. This ten-second check saves no recording.");
    void FlowBridge.testAudio().catch(error => { testMessage.textContent = String(error); }).finally(() => {
      testing = false;
      testButton.textContent = t("Test audio");
    });
  });
  const stopMeters = await onEvent<AudioSourceStatus[]>("audio-test-levels", levels => { if (page.isConnected) meters.update(levels); });
  const stopVisibility = await onEvent<boolean>("hub-visible", shown => {
    visible = shown;
    if (!shown) void FlowBridge.stopAudioTest();
  });
  ctx.onLeave = () => { stopMeters(); stopVisibility(); void FlowBridge.stopAudioTest(); };
  page.append(card(
    row(t("Call playback"), t("Automatic follows the call app's active audio devices, then Windows call defaults. Device choices stay fixed."), playbackSelect),
    h("div", { class: "hub-note", text: t("Bluetooth headset microphones can reduce playback quality. A laptop or USB microphone with Bluetooth headphones can keep playback clearer. Wired and USB headsets also work.") }),
    h("div", { class: "hub-note", text: t("Playback is captured digitally, including other apps using the selected output. Automatic devices reconnect during a call; manual choices stay fixed.") }),
    testButton, testMessage, meters.element,
  ));

  // ── Recognition ──
  page.append(label(t("Recognition")));
  page.append(card(
    row(t("Parakeet, on your computer"), t("The most accurate, offline, 25 languages. Downloaded once (about {0} MB).", { 0: number(status.models.downloadMb) }), modelsControl(ctx)),
    h("div", { class: "hub-note", text: t("Your voice never leaves this computer: Parakeet turns it into text here, and the recording is dropped as soon as it is pasted.") }),
  ));

  const performance = status.performance;
  const choices: [string, string][] = [["0", t("Automatic (measured on this computer)")]];
  for (let n = 1; n <= performance.availableThreads; n++) choices.push([String(n), t("CPU · {0} threads", { 0: n })]);
  const performanceText = h("div", { class: "hub-note" });
  const updatePerformance = (p: typeof performance) => {
    performanceText.textContent = p.error ?? (!p.automatic ? t("CPU · {0} threads", { 0: p.threads }) : p.tuning ? t("Measuring speech performance…")
      : p.measured ? t("Measured choice: CPU · {0} threads", { 0: p.threads })
      : t("Automatic tuning starts after a dictation of at least three seconds. Audio stays in memory and is never saved for tuning."));
  };
  updatePerformance(performance);
  page.append(card(
    row(t("Speech performance"), t("This build uses the CPU. Automatic mode measures supported thread counts and remembers the fastest result."),
      select(choices, String(prefs.speechThreads), value => void ctx.setPrefs(p => (p.speechThreads = Number(value))))),
    performanceText,
    button(t("Measure again"), () => void FlowBridge.retuneSpeech().then(() => ctx.refresh()).then(s => updatePerformance(s.performance)), "", true),
  ));
  const performanceTimer = window.setInterval(() => {
    if (!visible || !page.isConnected) return;
    if (page.isConnected && !testing) void FlowBridge.audioStatus().then(sources => { if (sources?.length) meters.update(sources); });
    if (page.isConnected) void FlowBridge.status().then(s => { if (s && page.isConnected) {
      updatePerformance(s.performance);
      updateAudioDevices(micSelect, s.mics, s.prefs.mic, s.platform === "windows");
      updateAudioDevices(playbackSelect, s.speakers, s.prefs.speakers, s.platform === "windows");
    } });
  }, 2000);
  const previousLeave = ctx.onLeave;
  ctx.onLeave = () => { window.clearInterval(performanceTimer); previousLeave?.(); };

  // ── Notetaker ──
  page.append(label(t("Notetaker")));
  page.append(card(
    row(t("Summaries"), null, select(textEngines(), prefs.summaryEngine, (engine) => void ctx.setPrefs((p) => (p.summaryEngine = engine)))),
    h("div", { class: "hub-note", text: t("Only the transcript goes to the provider you choose. Claude Code and Codex use your existing sign-in. Model servers use the address and model configured in Settings → Chat.") }),
    divider(),
    h("div", { class: "hub-note", text: t("Everything stays on this computer. Tell the people in the call that you are recording.") }),
  ));

  // ── App ──
  page.append(label(t("App")));
  page.append(card(
    row(t("Open this window when you open Coucou"), t("Not when Coucou starts with the computer."), toggle(prefs.openAtLaunch, (on) => void ctx.setPrefs((p) => (p.openAtLaunch = on)))),
  ));

  // ── Privacy ──
  page.append(label(t("Privacy")));
  const keep: [string, string][] = [["0", t("Don't keep")], ["7", t("7 days")], ["30", t("30 days")], ["365", t("1 year")], ["-1", t("Forever")]];
  const confirmRow = h("div", { class: "hub-row" });
  const erase = () => {
    clear(confirmRow);
    confirmRow.append(
      button(t("Erase the history"), () => void FlowBridge.clear(false), "", true),
      button(t("Erase everything"), () => {
        clear(confirmRow);
        confirmRow.append(
          h("span", { class: "hub-note", style: "flex:1", text: t("Erase the history and the statistics?") }),
          button(t("Erase everything"), () => {
            void FlowBridge.clear(true);
            erase();
          }, "danger", true),
          button(t("Cancel"), erase, "", true),
        );
      }, "", true),
    );
  };
  erase();
  page.append(card(
    row(t("Keep the history"), null, select(keep, String(prefs.keepDays), (v) => void ctx.setPrefs((p) => (p.keepDays = Number(v))))),
    divider(),
    confirmRow,
    h("div", { class: "hub-note", text: t("The history, the statistics and your notes stay on this computer, in Coucou's folder. Dictations are never saved as audio; a call's audio is deleted as soon as its transcript is written.") }),
  ));
}
