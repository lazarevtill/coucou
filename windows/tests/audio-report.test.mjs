import test from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { audioReport } from "../src/hub/audio.ts";

installFakeDom();

const source = { source: "microphone", device: "Laptop mic", devices: ["Laptop mic"],
  automatic: true, connected: true, level: 0, peak: 0, peakSeen: 0.5,
  clipping: false, received: true, error: null, warnings: [], sampleRate: 48000, channels: 2 };

test("a saved report retains sound between polls without showing a live recording", () => {
  const report = audioReport([source]);
  assert.match(report.textContent, /Laptop mic/);
  assert.match(report.textContent, /Audio received/);
  assert.match(report.textContent, /Peak level: -6 dBFS/);
  assert.match(report.textContent, /Last format: 48,000 Hz/);
  assert.doesNotMatch(report.textContent, /Recording continues|Waiting for sound|Following/);
  assert.equal(report.querySelector('[role="meter"]'), null);
});

test("saved diagnostics retain all used endpoints and open on capture trouble", () => {
  const report = audioReport([{ ...source, source: "playback", device: "Speakers",
    devices: ["Buds", "Speakers"], received: false, peakSeen: 0,
    warnings: ["Audio was interrupted. Some audio may be missing."] }]);
  assert.match(report.textContent, /Buds/);
  assert.match(report.textContent, /Speakers/);
  assert.match(report.textContent, /No audio received/);
  assert.match(report.textContent, /Some audio may be missing/);
  assert.equal(report.getAttribute("open"), "");
  assert.equal(report.textContent.match(/Speakers/g).length, 1);
});

test("device names and errors remain text even if a driver returns markup", () => {
  const report = audioReport([{ ...source, device: '<img src=x onerror="bad()">', devices: [], error: "<script>bad()</script>" }]);
  assert.match(report.textContent, /<img src=x/);
  assert.match(report.textContent, /<script>/);
  assert.equal(report.querySelector("img"), null);
  assert.equal(report.querySelector("script"), null);
});
