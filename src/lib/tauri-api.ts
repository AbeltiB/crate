import { invoke } from "@tauri-apps/api/core";

export interface MediaItem {
  id: string;
  title: string;
  url: string;
  duration: number | null;
  thumbnail: string | null;
  index: number;
}

export interface PlaylistInfo {
  title: string;
  items: MediaItem[];
}

export type DownloadStatus = "downloading" | "processing" | "done" | "failed";

export interface DownloadProgress {
  itemId: string;
  status: DownloadStatus;
  percent: number | null;
  error: string | null;
}

export function analyzePlaylist(url: string) {
  return invoke<PlaylistInfo>("analyze_playlist", { url });
}

export function downloadItems(playlistTitle: string, items: MediaItem[]) {
  return invoke<void>("download_items", { playlistTitle, items });
}
