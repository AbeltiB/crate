# Playlist Audio Manager — Architecture & Phased Development Plan
**Windows-only, zero-cost stack. Personal tool for now, solo build. Phase 0 → full application.**

**Scope confirmed:** personal use only (distribution/monetization is an undecided long-future maybe, not a current driver), solo development. Core ask: paste a YouTube playlist URL, hit run, get near-zero failed downloads through robust retries, and never re-download or duplicate an item. That reframes **Phases 0–3 as the actual MVP** — everything from Phase 4 onward is a later, optional add-on, not a requirement to get to a usable personal tool. Distribution polish (§3, Phase 7) drops to low priority since there's no one else installing this yet.

---

## 1. Final Stack Decision

| Layer | Choice | Why (and why not the alternative) |
|---|---|---|
| Desktop shell | **Tauri v2** | Uses the OS's built-in WebView2 on Windows — no bundled Chromium like Electron, so installers stay ~10–15MB instead of ~100MB+, RAM footprint is far lower, and there's zero licensing cost either way. Since you're comfortable with Rust, there's no real reason to trade this down to Electron anymore. |
| Frontend | React + TypeScript + Vite | Matches your primary stack directly. |
| Styling | Tailwind CSS + shadcn/ui | Free, unstyled-primitive based (Radix), no design-system lock-in. |
| Client state | Zustand | Lightweight, matches what you already reach for. |
| Async data layer | TanStack Query | Wraps `invoke()` calls to Rust commands with caching/retry/loading states — even with no server, this gives you a clean pattern for "fetch playlist," "fetch history," etc. |
| Backend runtime | **Rust + Tokio** | Async runtime for process orchestration and the worker pool. |
| Local DB | **SQLite via `rusqlite` + `r2d2` pool** | Simpler and more mature than `sqlx` for this workload; a pooled connection handles concurrent writes from multiple download workers cleanly. Migrations via `refinery` (compiled in, no external tool needed). |
| Process orchestration | `tokio::process::Command` | Spawns yt-dlp.exe / ffmpeg.exe, streams stdout via `--progress-template` (structured JSON progress lines — far more robust than regex-parsing human-readable output). |
| Download engine | **yt-dlp** (static Windows build) | Free, open-source, actively maintained. |
| Media processing | **FFmpeg** (static Windows build, e.g. gyan.dev or BtbN builds) | Free. |
| Packaging | Tauri bundler → **portable .exe first, NSIS installer second** | Both free. Portable-first sidesteps some install-time SmartScreen friction (see §3). |
| Auto-update | Tauri's built-in updater plugin, **self-hosted via GitHub Releases** | Free — no update server needed. |
| CI/CD | GitHub Actions | Free tier covers Windows build minutes for a project this size. |
| Error handling | `anyhow` + `thiserror` | Standard, zero-cost. |

**Nothing in this stack has a subscription, license fee, or paid tier.**

---

## 2. Architecture

```
┌──────────────────────────────────────────────┐
│                  Tauri App                   │
│                                              │
│  React + TypeScript UI                       │
│  (Zustand state, TanStack Query, shadcn/ui)  │
│              │  invoke() / events            │
│              ▼                                │
│         Tauri Commands (Rust)                │
│              │                                │
│              ▼                                │
│      Application Layer (Rust, Tokio)         │
│   ┌──────────────┬──────────────┐            │
│   │ Job Manager  │ Worker Pool  │            │
│   │ (state       │ (Semaphore-  │            │
│   │  machine)    │  bounded)    │            │
│   └──────┬───────┴──────┬───────┘            │
│          ▼               ▼                    │
│   SQLite (rusqlite)   Process Spawner         │
│                            │                   │
│                     ┌──────┴──────┐            │
│                     ▼             ▼            │
│                 yt-dlp.exe    ffmpeg.exe       │
│                     │             │            │
│                     └──────┬──────┘            │
│                            ▼                    │
│                   Local filesystem              │
└──────────────────────────────────────────────┘
```

Progress flows back via an `mpsc` channel from each worker to a central aggregator, which writes to SQLite and emits Tauri events (`app.emit`) that the React side subscribes to — no polling.

---

## 3. Zero-Cost Windows Distribution — the honest version (low priority right now)

Since this is running only on your own machine for now, SmartScreen friction barely matters — you'll click "Run anyway" once and move on. This section only becomes relevant if the "maybe distribute someday" scenario ever materializes, so treat it as reference, not a task:

- **Portable build.** Tauri can produce a standalone `.exe` with no installer — simplest for a single personal machine, nothing to install/uninstall.
- There is **no free way to fully eliminate the SmartScreen "Unknown Publisher" warning** for other users if you do distribute later. Winget/Chocolatey submission (free) and organic reputation building are the zero-cost mitigations; a paid EV cert (~$300–500/yr) is the only way to remove it outright, and only worth considering if monetization actually happens.

---

## 4. Fixed Database Schema

Corrects the three issues flagged earlier: no unique constraint enabling true dedupe, no retry bookkeeping, and job/item status able to drift out of sync.

```sql
CREATE TABLE playlists (
  id INTEGER PRIMARY KEY,
  source TEXT NOT NULL,              -- e.g. 'youtube'
  source_playlist_id TEXT NOT NULL,
  title TEXT,
  channel TEXT,
  url TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(source, source_playlist_id)
);

CREATE TABLE media_items (
  id INTEGER PRIMARY KEY,
  playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
  source TEXT NOT NULL,
  source_id TEXT NOT NULL,           -- video ID
  title TEXT,
  url TEXT NOT NULL,
  duration INTEGER,
  position INTEGER,
  thumbnail_url TEXT,
  status TEXT NOT NULL DEFAULT 'DISCOVERING',
  UNIQUE(source, source_id)          -- <-- enforces real dedupe at the DB layer
);
CREATE INDEX idx_media_items_playlist_status ON media_items(playlist_id, status);

CREATE TABLE downloads (
  id INTEGER PRIMARY KEY,
  media_item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
  format TEXT NOT NULL,
  quality TEXT NOT NULL,
  file_path TEXT,
  file_size INTEGER,
  status TEXT NOT NULL DEFAULT 'QUEUED',
  attempt_count INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  next_retry_at TEXT,                -- <-- enables the 3-attempt backoff policy
  started_at TEXT,
  completed_at TEXT
);
CREATE INDEX idx_downloads_media_item ON downloads(media_item_id);
CREATE INDEX idx_downloads_status ON downloads(status);

CREATE TABLE jobs (
  id INTEGER PRIMARY KEY,
  playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
  status TEXT NOT NULL DEFAULT 'READY',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  completed_at TEXT
  -- total/completed/failed counts are DERIVED at read time from media_items,
  -- never stored independently — eliminates the drift risk flagged earlier
);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

---

## 5. Fixed State Machine

Adds the missing `PAUSED` state with correct in/out transitions.

```
DISCOVERING → READY → QUEUED → DOWNLOADING → PROCESSING → COMPLETED
                         │           │
                         ▼           ▼
                      PAUSED ◄───────┘
                         │
                         ▼
                     (resume → QUEUED/DOWNLOADING)

DOWNLOADING → FAILED → RETRYING → DOWNLOADING   (up to attempt_count = 3)
FAILED (attempts exhausted) → terminal

QUEUED / PAUSED / FAILED → CANCELLED             (terminal, not resumable)
```

Job-level status = derived aggregate of its `media_items` statuses (e.g. all `COMPLETED` → job `COMPLETED`; any `DOWNLOADING` → job `DOWNLOADING`), computed on read, never written directly.

---

## 6. Concurrency Model

```rust
let semaphore = Arc::new(Semaphore::new(concurrency)); // default 2, user setting 1–4

for item in queued_items {
    let permit = semaphore.clone().acquire_owned().await?;
    let tx = progress_tx.clone();
    tokio::spawn(async move {
        let _permit = permit; // held for the duration of the download
        download_item(item, tx).await;
    });
}
```

A single aggregator task owns the SQLite writes (via the `r2d2` pool) and re-emits progress to the frontend — workers never touch the DB directly, avoiding write contention and keeping the state machine transitions in one place.

---

## 7. Phase-Based Roadmap (Phase 0 → full application)

Estimates assume solo-dev pace; treat as ordering, not a deadline commitment.

### 🎯 Personal MVP = Phases 0–3. This is the actual target: paste URL, run, near-zero failures, no duplicates. Phases 4+ are optional enhancements to pick up later, in whatever order you want, only if you feel like it.

### Phase 0 — Foundations (~1–2 weeks)
- Tauri v2 scaffold; Rust workspace; React + TS + Tailwind + shadcn wired end-to-end
- **Dependency manager**: on first launch, download static yt-dlp.exe + ffmpeg.exe into the app data dir, verify checksums — no CLI configuration ever shown to the user
- Prove one round-trip: React `invoke()` → Rust command → response

### Phase 1 — Core Engine Proof (MVP core, ~2–3 weeks)
- Paste URL → analyze via `yt-dlp --flat-playlist --dump-json`
- Item list (non-virtualized is fine at this stage), select all / individual
- Single format (MP3), single quality (best), fixed path `Music/{playlist}/{index} - {title}.mp3`
- Sequential (concurrency = 1) download → ffmpeg extract → write file, proving the full pipeline
- No DB yet — in-memory state is fine here

### Phase 2 — Queue & Robustness (~2 weeks)
- SQLite schema from §4 + `refinery` migrations
- Bounded worker pool (§6), default concurrency 2
- Real per-item progress via `--progress-template` JSON parsing
- Retry policy: 3 attempts, exponential backoff using `attempt_count` / `next_retry_at`
- A single item failing never halts the job
- Pause / Resume / Cancel wired to the fixed state machine (§5)

### Phase 3 — Duplicate Detection & Smart Sync (~1–2 weeks)
- Archive lookup (`UNIQUE(source, source_id)`) before queuing anything
- "Sync Playlist": re-analyze → diff against archive → "120 downloaded / 7 new"
- Dedicated, tested filename-sanitization module (path traversal, reserved Windows device names, illegal chars)

---
**Everything below this line is optional — nice-to-have, pick up later if/when you want it. The personal MVP is done at Phase 3.**

### Phase 4 — Metadata & Organization (optional, later)
- ID3 tag embedding via `ffmpeg -metadata` (title/artist/album/track)
- Thumbnail → embedded album art
- Folder-org modes: playlist / channel / flat (user setting)
- Filename template picker

### Phase 5 — History, Filters, Settings (optional, later)
- History screen backed by `jobs` + `downloads`
- Filters (search, duration, status) over a **virtualized** list (TanStack Virtual) — this is where item counts start to matter
- Settings screen: concurrency, retry count, speed-limit stub, default format/quality, output folder

### Phase 6 — Performance & Scale Hardening (optional, later)
- Confirm smooth UI at 1,000+ item playlists
- Speed limiting via yt-dlp/ffmpeg rate flags
- Diagnostics export (app + engine versions, last N log lines) — strip anything session-sensitive before export

### Phase 7 — Polish & Windows Release (optional, later — matters only if you ever distribute)
- Dark mode, native notifications (Tauri notification plugin)
- Portable build + NSIS installer (§3)
- Self-hosted auto-update via Tauri updater plugin + GitHub Releases
- Winget manifest submission

### Phase 8 — Library View (optional, later)
- Local library browser: artists / albums / playlists / recently added
- Optional embedded audio player

### Phase 9 — Multi-Source Abstraction (future)
- `MediaProvider` trait so new sources plug in without touching core logic
- Only add sources with a clear, explicit authorized-download policy

---

## Where to start
Phase 0 → Phase 1 → Phase 2 → Phase 3, in order — that's the whole personal MVP. Say the word when you want to start scaffolding Phase 0 (Tauri workspace, dependency manager for yt-dlp/ffmpeg) and I'll build it out as real code.
