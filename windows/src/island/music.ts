import { Bridge, onEvent } from "../core/bridge";
import { MUSIC_ID, Music } from "../core/media";
import { State } from "../core/state";
import type { SpotifyState } from "../core/spotify";
import type { SpotifyHost } from "./spotify";

function syncName() {
  const task = State.tasks.find(task => task.id === MUSIC_ID);
  if (task) task.name = Music.state.track?.title || "Apple Music";
}

export function applyMusic(island: SpotifyHost, next: SpotifyState) {
  const wasPlaying = State.musicPlaying;
  Music.state = next;
  syncName();
  if (!wasPlaying && State.musicPlaying && !State.paused && State.mode === "hidden") island.revealSilently();
  State.notify();
}

export function registerMusicHandlers(island: SpotifyHost) {
  void onEvent<SpotifyState>("apple-music", state => applyMusic(island, state));
  void onEvent<{ artUrl: string; dataUrl: string }>("apple-music-artwork", art => {
    Music.artwork = art;
    State.notify();
  });
  let shown = false;
  State.subscribe(() => {
    syncName();
    const now = State.mode === "expanded" && State.view === "overview" && State.focusTask?.id === MUSIC_ID;
    if (now && !shown) void Bridge.musicRefresh();
    shown = now;
  });
}
