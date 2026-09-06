import { useEffect, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  analyzePlaylist,
  cancelJob,
  pauseJob,
  resumeJob,
  startDownloadJob,
  type DownloadProgress,
} from "@/lib/tauri-api";
import { usePlaylistStore } from "@/stores/playlist-store";

function formatDuration(seconds: number | null) {
  if (seconds == null) return "--:--";
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function statusLabel(status: string, percent: number | null) {
  switch (status) {
    case "READY":
      return "";
    case "QUEUED":
      return "queued";
    case "DOWNLOADING":
      return `downloading${percent != null ? " " + Math.round(percent) + "%" : ""}`;
    case "PROCESSING":
      return "processing…";
    case "RETRYING":
      return "retrying…";
    case "COMPLETED":
      return "done";
    case "FAILED":
      return "failed";
    case "CANCELLED":
      return "cancelled";
    default:
      return status.toLowerCase();
  }
}

function statusColor(status: string) {
  if (status === "COMPLETED") return "text-signal";
  if (status === "FAILED") return "text-oxblood";
  if (status === "RETRYING") return "text-wax";
  return "text-slate";
}

function App() {
  const [urlInput, setUrlInput] = useState("");
  const {
    playlistTitle,
    items,
    jobId,
    jobPaused,
    setAnalyzed,
    toggleItem,
    toggleAll,
    applyProgress,
    setJobId,
    setJobPaused,
  } = usePlaylistStore();

  useEffect(() => {
    const unlisten = listen<DownloadProgress>("download-progress", (event) => {
      applyProgress(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [applyProgress]);

  const analyzeMutation = useMutation({
    mutationFn: (url: string) => analyzePlaylist(url),
    onSuccess: (data) => setAnalyzed(data.sourcePlaylistId, data.title, data.items),
  });

  const downloadMutation = useMutation({
    mutationFn: () => {
      const state = usePlaylistStore.getState();
      const selectedIds = state.items.filter((i) => i.selected).map((i) => i.id);
      return startDownloadJob(
        {
          sourcePlaylistId: state.sourcePlaylistId ?? "",
          url: urlInput,
          title: state.playlistTitle ?? "Playlist",
          items: state.items,
        },
        selectedIds,
      );
    },
    onSuccess: (newJobId) => setJobId(newJobId),
  });

  const selectedCount = items.filter((item) => item.selected).length;
  const isRunning = jobId != null;

  return (
    <main className="min-h-screen px-8 py-10">
      <h1 className="font-display text-5xl italic text-vellum">Crate</h1>

      <form
        className="mt-8 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (urlInput.trim()) analyzeMutation.mutate(urlInput.trim());
        }}
      >
        <input
          type="text"
          value={urlInput}
          onChange={(e) => setUrlInput(e.target.value)}
          placeholder="Paste a YouTube playlist URL"
          className="flex-1 rounded border border-slate/30 bg-ink2 px-3 py-2 font-sans text-sm text-vellum outline-none focus:border-wax"
        />
        <button
          type="submit"
          disabled={analyzeMutation.isPending}
          className="rounded bg-wax px-4 py-2 font-sans text-sm font-medium text-ink disabled:opacity-50"
        >
          {analyzeMutation.isPending ? "Analyzing…" : "Analyze"}
        </button>
      </form>

      {analyzeMutation.isError && (
        <p className="mt-3 font-mono text-sm text-oxblood">{String(analyzeMutation.error)}</p>
      )}

      {items.length > 0 && (
        <section className="mt-8">
          <div className="flex items-center justify-between">
            <h2 className="font-sans text-lg text-vellum">
              {playlistTitle} <span className="text-slate">· {items.length} items</span>
              {items.some((i) => i.alreadyDownloaded) && (
                <span className="ml-2 font-mono text-xs text-signal">
                  {items.filter((i) => i.alreadyDownloaded).length} downloaded ·{" "}
                  {items.filter((i) => !i.alreadyDownloaded).length} new
                </span>
              )}
            </h2>
            <div className="flex gap-3">
              <button
                onClick={() => toggleAll(true)}
                disabled={isRunning}
                className="font-mono text-xs text-slate hover:text-vellum disabled:opacity-40"
              >
                Select all
              </button>
              <button
                onClick={() => toggleAll(false)}
                disabled={isRunning}
                className="font-mono text-xs text-slate hover:text-vellum disabled:opacity-40"
              >
                Select none
              </button>
            </div>
          </div>

          <ul className="mt-4 divide-y divide-slate/15">
            {items.map((item) => (
              <li key={item.id} className="flex items-center gap-3 py-2">
                <input
                  type="checkbox"
                  checked={item.selected}
                  onChange={() => toggleItem(item.id)}
                  disabled={isRunning}
                  className="accent-wax"
                />
                <span className="flex-1 truncate font-sans text-sm text-vellum">
                  {item.index}. {item.title}
                </span>
                <span className="font-mono text-xs text-slate">{formatDuration(item.duration)}</span>
                <span className={`w-32 text-right font-mono text-xs ${statusColor(item.status)}`}>
                  {statusLabel(item.status, item.percent)}
                </span>
              </li>
            ))}
          </ul>

          <div className="mt-4 flex items-center gap-3">
            {!isRunning && (
              <button
                onClick={() => downloadMutation.mutate()}
                disabled={selectedCount === 0 || downloadMutation.isPending}
                className="rounded bg-wax px-4 py-2 font-sans text-sm font-medium text-ink disabled:opacity-50"
              >
                {downloadMutation.isPending ? "Starting…" : `Download selected (${selectedCount})`}
              </button>
            )}

            {isRunning && !jobPaused && (
              <button
                onClick={() => {
                  pauseJob(jobId);
                  setJobPaused(true);
                }}
                className="rounded border border-slate/30 px-4 py-2 font-sans text-sm text-vellum"
              >
                Pause
              </button>
            )}

            {isRunning && jobPaused && (
              <button
                onClick={() => {
                  resumeJob(jobId);
                  setJobPaused(false);
                }}
                className="rounded bg-wax px-4 py-2 font-sans text-sm font-medium text-ink"
              >
                Resume
              </button>
            )}

            {isRunning && (
              <button
                onClick={() => {
                  cancelJob(jobId);
                  setJobId(null);
                }}
                className="rounded border border-oxblood/50 px-4 py-2 font-sans text-sm text-oxblood"
              >
                Cancel
              </button>
            )}
          </div>

          {downloadMutation.isError && (
            <p className="mt-3 font-mono text-sm text-oxblood">{String(downloadMutation.error)}</p>
          )}
        </section>
      )}
    </main>
  );
}

export default App;
