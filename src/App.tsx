import { useEffect, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { analyzePlaylist, downloadItems, type DownloadProgress } from "@/lib/tauri-api";
import { usePlaylistStore } from "@/stores/playlist-store";

function formatDuration(seconds: number | null) {
  if (seconds == null) return "--:--";
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function App() {
  const [urlInput, setUrlInput] = useState("");
  const { playlistTitle, items, setAnalyzed, toggleItem, toggleAll, applyProgress } =
    usePlaylistStore();

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
    onSuccess: (data) => setAnalyzed(data.title, data.items),
  });

  const downloadMutation = useMutation({
    mutationFn: () => {
      const selected = items.filter((item) => item.selected);
      return downloadItems(playlistTitle ?? "Playlist", selected);
    },
  });

  const selectedCount = items.filter((item) => item.selected).length;

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
        <p className="mt-3 font-mono text-sm text-oxblood">
          {String(analyzeMutation.error)}
        </p>
      )}

      {items.length > 0 && (
        <section className="mt-8">
          <div className="flex items-center justify-between">
            <h2 className="font-sans text-lg text-vellum">
              {playlistTitle} <span className="text-slate">· {items.length} items</span>
            </h2>
            <div className="flex gap-3">
              <button
                onClick={() => toggleAll(true)}
                className="font-mono text-xs text-slate hover:text-vellum"
              >
                Select all
              </button>
              <button
                onClick={() => toggleAll(false)}
                className="font-mono text-xs text-slate hover:text-vellum"
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
                  className="accent-wax"
                />
                <span className="flex-1 truncate font-sans text-sm text-vellum">
                  {item.index}. {item.title}
                </span>
                <span className="font-mono text-xs text-slate">
                  {formatDuration(item.duration)}
                </span>
                <span className="w-28 text-right font-mono text-xs text-slate">
                  {item.status === "idle" && ""}
                  {item.status === "downloading" &&
                    `downloading ${item.percent != null ? Math.round(item.percent) + "%" : ""}`}
                  {item.status === "processing" && "processing…"}
                  {item.status === "done" && <span className="text-signal">done</span>}
                  {item.status === "failed" && <span className="text-oxblood">failed</span>}
                </span>
              </li>
            ))}
          </ul>

          <button
            onClick={() => downloadMutation.mutate()}
            disabled={selectedCount === 0 || downloadMutation.isPending}
            className="mt-4 rounded bg-wax px-4 py-2 font-sans text-sm font-medium text-ink disabled:opacity-50"
          >
            {downloadMutation.isPending
              ? "Downloading…"
              : `Download selected (${selectedCount})`}
          </button>

          {downloadMutation.isError && (
            <p className="mt-3 font-mono text-sm text-oxblood">
              {String(downloadMutation.error)}
            </p>
          )}
        </section>
      )}
    </main>
  );
}

export default App;
