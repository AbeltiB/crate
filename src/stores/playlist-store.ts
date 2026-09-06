import { create } from "zustand";
import type { DownloadProgress, ItemStatus, MediaItem } from "@/lib/tauri-api";

export interface PlaylistItem extends MediaItem {
  selected: boolean;
  status: ItemStatus;
  percent: number | null;
  error: string | null;
}

interface PlaylistState {
  url: string;
  sourcePlaylistId: string | null;
  playlistTitle: string | null;
  items: PlaylistItem[];
  jobId: number | null;
  jobPaused: boolean;
  setUrl: (url: string) => void;
  setAnalyzed: (sourcePlaylistId: string, title: string, items: MediaItem[]) => void;
  toggleItem: (id: string) => void;
  toggleAll: (selected: boolean) => void;
  applyProgress: (progress: DownloadProgress) => void;
  setJobId: (jobId: number | null) => void;
  setJobPaused: (paused: boolean) => void;
}

export const usePlaylistStore = create<PlaylistState>((set) => ({
  url: "",
  sourcePlaylistId: null,
  playlistTitle: null,
  items: [],
  jobId: null,
  jobPaused: false,

  setUrl: (url) => set({ url }),

  setAnalyzed: (sourcePlaylistId, title, items) =>
    set({
      sourcePlaylistId,
      playlistTitle: title,
      jobId: null,
      jobPaused: false,
      items: items.map((item) => ({
        ...item,
        selected: !item.alreadyDownloaded,
        status: item.alreadyDownloaded ? "COMPLETED" : "READY",
        percent: null,
        error: null,
      })),
    }),

  toggleItem: (id) =>
    set((state) => ({
      items: state.items.map((item) =>
        item.id === id ? { ...item, selected: !item.selected } : item,
      ),
    })),

  toggleAll: (selected) =>
    set((state) => ({
      items: state.items.map((item) => ({ ...item, selected })),
    })),

  applyProgress: (progress) =>
    set((state) => ({
      items: state.items.map((item) =>
        item.id === progress.sourceId
          ? { ...item, status: progress.status, percent: progress.percent, error: progress.error }
          : item,
      ),
    })),

  setJobId: (jobId) => set({ jobId, jobPaused: false }),
  setJobPaused: (jobPaused) => set({ jobPaused }),
}));
