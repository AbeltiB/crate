CREATE TABLE playlists (
  id INTEGER PRIMARY KEY,
  source TEXT NOT NULL,
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
  source_id TEXT NOT NULL,
  title TEXT,
  url TEXT NOT NULL,
  duration INTEGER,
  position INTEGER,
  thumbnail_url TEXT,
  status TEXT NOT NULL DEFAULT 'DISCOVERING',
  UNIQUE(source, source_id)
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
  next_retry_at TEXT,
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
);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
