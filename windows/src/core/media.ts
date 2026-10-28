import { Bridge } from "./bridge";
import { Spotify, IDLE_SPOTIFY, SPOTIFY_ID, SPOTIFY_GREEN, type SpotifyState } from "./spotify";

export const MUSIC_ID = "integration_music";
export const Music = { state: { ...IDLE_SPOTIFY }, artwork: null as { artUrl: string; dataUrl: string } | null };

export interface MediaPlayer {
  id: string;
  name: string;
  color: string;
  store: { state: SpotifyState; artwork: { artUrl: string; dataUrl: string } | null };
  control(action: string, value?: number): Promise<boolean | null>;
  open(): Promise<boolean | null>;
  refresh(): Promise<SpotifyState | null>;
}

export const spotifyPlayer: MediaPlayer = {
  id: SPOTIFY_ID, name: "Spotify", color: SPOTIFY_GREEN, store: Spotify,
  control: Bridge.spotifyControl, open: Bridge.spotifyOpen, refresh: Bridge.spotifyRefresh,
};
export const appleMusicPlayer: MediaPlayer = {
  id: MUSIC_ID, name: "Apple Music", color: "#FA2D48", store: Music,
  control: Bridge.musicControl, open: Bridge.musicOpen, refresh: Bridge.musicRefresh,
};

export function playerArtwork(player: MediaPlayer): string | null {
  const art = player.store.artwork;
  return art && player.store.state.track?.artUrl === art.artUrl ? art.dataUrl : null;
}
