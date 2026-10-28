import test from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { audioMeters, updateAudioDevices } from "../src/hub/audio.ts";

installFakeDom();

test("an audio interruption stays visible while a recovered source continues recording", () => {
  const meters = audioMeters();
  meters.update([{ source: "microphone", device: "USB Headset · 00000001", automatic: false,
    connected: true, level: 0.1, peak: 0.2, clipping: false, received: true, error: null,
    warnings: ["Audio was interrupted. Recording continues."], sampleRate: 48000, channels: 2 }]);
  assert.match(meters.element.textContent, /USB Headset · 00000001/);
  assert.match(meters.element.textContent, /Sound detected/);
  assert.match(meters.element.textContent, /Audio was interrupted/);
  assert.doesNotMatch(meters.element.textContent, /Unavailable/);
});

test("disconnected sources report unavailable and their retained device names", () => {
  const meters = audioMeters();
  meters.update([{ source: "playback", device: "Buds", automatic: false, connected: false,
    level: 0, peak: 0, clipping: false, received: true, error: null, warnings: [], sampleRate: 48000, channels: 2 }]);
  assert.match(meters.element.textContent, /Buds/);
  assert.match(meters.element.textContent, /Unavailable/);
  assert.doesNotMatch(meters.element.textContent, /Sound detected/);
});

test("hotplug choices retain a manual ID and replace its disconnected label on return", () => {
  const picker = document.createElement("select");
  updateAudioDevices(picker, [], "buds-id", true);
  assert.equal(picker.value, "buds-id");
  assert.match(picker.textContent, /disconnected/);
  updateAudioDevices(picker, [{ id: "buds-id", name: "Buds", default: false }], "buds-id", true);
  assert.equal(picker.value, "buds-id");
  assert.match(picker.textContent, /Buds/);
  assert.doesNotMatch(picker.textContent, /disconnected/);
  assert.match(picker.textContent, /Automatic/);
});

test("the audio meter names the app that supplied its automatic route", () => {
  const meters = audioMeters();
  meters.update([{ source: "playback", device: "Buds", application: "Zoom", automatic: true,
    connected: true, level: 0.1, peak: 0.2, clipping: false, received: true, error: null, sampleRate: 48000, channels: 2 }]);
  assert.match(meters.element.textContent, /Following Zoom/);
});
