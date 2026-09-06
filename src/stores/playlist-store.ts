import { create } from "zustand";
import type { DownloadProgress, DownloadStatus, MediaItem } from "@/lib/tauri-api";

export interface PlaylistItem extends MediaItem {
  selected: boolean;
  status: DownloadStatus | "idle";
  percent: number | null;
  error: string | null;
}

interface PlaylistState {
  url: string;
  playlistTitle: string | null;
  items: PlaylistItem[];
  setUrl: (url: string) => void;
  setAnalyzed: (title: string, items: MediaItem[]) => void;
  toggleItem: (id: string) => void;
  toggleAll: (selected: boolean) => void;
  applyProgress: (progress: DownloadProgress) => void;
}

export const usePlaylistStore = create<PlaylistState>((set) => ({
  url: "",
  playlistTitle: null,
  items: [],

  setUrl: (url) => set({ url }),

  setAnalyzed: (title, items) =>
    set({
      playlistTitle: title,
      items: items.map((item) => ({
        ...item,
        selected: true,
        status: "idle",
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
        item.id === progress.itemId
          ? { ...item, status: progress.status, percent: progress.percent, error: progress.error }
          : item,
      ),
    })),
}));
