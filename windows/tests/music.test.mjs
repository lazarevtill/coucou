import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { calls, sent, emit } from "./tauri.mjs";
import { appleMusicPlayer, MUSIC_ID, Music, playerArtwork } from "../src/core/media.ts";
import { Spotify, IDLE_SPOTIFY, islandDances } from "../src/core/spotify.ts";
import { State, DEFAULT_SETTINGS } from "../src/core/state.ts";
import { buildSpotifyCard, buildSpotifyPill } from "../src/views/spotify.ts";
import { registerMusicHandlers } from "../src/island/music.ts";

installFakeDom();
const track = { id: "apple-music:track:1", title: "Song", artist: "Artist", album: "Album", duration: 180, artUrl: "apple-art" };
beforeEach(() => {
  calls.length = 0;
  Music.state = { ...IDLE_SPOTIFY, running: true, installed: true, track, playing: true };
  Music.artwork = null;
  Spotify.state = { ...IDLE_SPOTIFY };
  Spotify.artwork = null;
  State.os = "windows";
  State.settings = { ...DEFAULT_SETTINGS, activeIntegrations: [MUSIC_ID] };
  State.tasks = [];
  State.mode = "compact";
});

test("Apple Music controls and artwork never target Spotify", () => {
  const card = buildSpotifyCard(appleMusicPlayer);
  card.sync();
  assert.equal(card.el.querySelector(".np-title").textContent, "Song");
  card.el.querySelector(".np-play").fire("click");
  assert.deepEqual(sent("music_control"), [{ action: "playPause", value: null }]);
  assert.deepEqual(sent("spotify_control"), []);
  assert.equal(Spotify.state.playing, false);
  Spotify.artwork = { artUrl: "apple-art", dataUrl: "wrong-player" };
  assert.equal(playerArtwork(appleMusicPlayer), null);
  Music.artwork = { artUrl: "apple-art", dataUrl: "data:image/png;base64,a" };
  assert.equal(playerArtwork(appleMusicPlayer), "data:image/png;base64,a");
});

test("Apple Music events name only their pill and drive the shared dance", async () => {
  const task = { id: MUSIC_ID, name: "Apple Music", color: "#FA2D48", state: "idle", steps: [] };
  State.tasks = [task];
  let reveals = 0;
  Music.state = { ...IDLE_SPOTIFY };
  State.mode = "hidden";
  registerMusicHandlers({ revealSilently() { reveals++; } });
  await new Promise(resolve => setTimeout(resolve, 0));
  emit("apple-music", { ...IDLE_SPOTIFY, running: true, installed: true, playing: true, track });
  assert.equal(task.name, "Song");
  assert.equal(reveals, 1);
  assert.equal(State.musicPlaying, true);
  assert.equal(State.spotifyPlaying, false);
  assert.equal(islandDances({ music: true, state: "idle", mode: "expanded", view: "overview", focusId: MUSIC_ID }), true);
  const pill = buildSpotifyPill(task, () => {}, appleMusicPlayer);
  pill.el.find(".np-pill-btn")[1].fire("click");
  assert.deepEqual(sent("music_control").at(-1), { action: "next", value: null });
});
