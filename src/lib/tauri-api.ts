import { invoke } from "@tauri-apps/api/core";

export interface MediaItem {
  id: string;
  title: string;
  url: string;
  duration: number | null;
  thumbnail: string | null;
  index: number;
  alreadyDownloaded: boolean;
}

export interface PlaylistInfo {
  sourcePlaylistId: string;
  url: string;
  title: string;
  items: MediaItem[];
}

export type ItemStatus =
  | "DISCOVERING"
  | "READY"
  | "QUEUED"
  | "DOWNLOADING"
  | "PROCESSING"
  | "COMPLETED"
  | "PAUSED"
  | "RETRYING"
  | "FAILED"
  | "CANCELLED";

export interface DownloadProgress {
  mediaItemId: number;
  sourceId: string;
  status: ItemStatus;
  percent: number | null;
  error: string | null;
}

export function analyzePlaylist(url: string) {
  return invoke<PlaylistInfo>("analyze_playlist", { url });
}

export function startDownloadJob(playlist: PlaylistInfo, selectedIds: string[]) {
  return invoke<number>("start_download_job", { playlist, selectedIds });
}

export function pauseJob(jobId: number) {
  return invoke<void>("pause_job", { jobId });
}

export function resumeJob(jobId: number) {
  return invoke<void>("resume_job", { jobId });
}

export function cancelJob(jobId: number) {
  return invoke<void>("cancel_job", { jobId });
}
