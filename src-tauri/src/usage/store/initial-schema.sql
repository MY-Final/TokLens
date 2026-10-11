
CREATE TABLE source_files (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    client           TEXT    NOT NULL,
    path             TEXT    NOT NULL,
    kind             TEXT    NOT NULL CHECK (kind IN ('jsonl', 'sqlite')),
    size_bytes       INTEGER,
    mtime_ms         INTEGER,
    content_offset   INTEGER NOT NULL DEFAULT 0,
    parser_version   INTEGER NOT NULL,
    last_scan_ms     INTEGER,
    unparsed_records INTEGER NOT NULL DEFAULT 0,
    UNIQUE (client, path)
);

CREATE TABLE sessions (
    id                       TEXT PRIMARY KEY,
    client                   TEXT    NOT NULL,
    client_session_id        TEXT    NOT NULL,
    parent_client_session_id TEXT,
    project_path             TEXT,
    model                    TEXT,
    started_at_ms            INTEGER,
    ended_at_ms              INTEGER,
    semantics_id             TEXT    NOT NULL,
    total_basis              TEXT    NOT NULL CHECK (total_basis IN ('client_cumulative', 'client_reported', 'sum_of_messages')),
    message_coverage         TEXT    NOT NULL CHECK (message_coverage IN ('complete', 'partial', 'none')),
    source_kind              TEXT    NOT NULL CHECK (source_kind IN ('measured', 'derived', 'estimated')),
    input_tokens             INTEGER,
    output_tokens            INTEGER,
    reasoning_tokens         INTEGER,
    cache_read_tokens        INTEGER,
    cache_write_tokens       INTEGER,
    total_tokens             INTEGER,
    client_total_tokens      INTEGER,
    client_total_mismatch    INTEGER NOT NULL DEFAULT 0 CHECK (client_total_mismatch IN (0, 1)),
    unknown_relations        TEXT,
    cost_source              TEXT    NOT NULL CHECK (cost_source IN ('client_reported', 'none')),
    client_cost_usd          REAL,
    imported_at_ms           INTEGER NOT NULL,
    updated_at_ms            INTEGER NOT NULL,
    UNIQUE (client, client_session_id)
);

CREATE INDEX idx_sessions_client_started ON sessions (client, started_at_ms);
CREATE INDEX idx_sessions_project_started ON sessions (project_path, started_at_ms);

CREATE TABLE messages (
    id                    TEXT PRIMARY KEY,
    session_id            TEXT    NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    client                TEXT    NOT NULL,
    dedup_key             TEXT    NOT NULL,
    seq                   INTEGER,
    created_at_ms         INTEGER,
    completed_at_ms       INTEGER,
    model                 TEXT,
    agent                 TEXT,
    semantics_id          TEXT    NOT NULL,
    source_kind           TEXT    NOT NULL CHECK (source_kind IN ('measured', 'derived', 'estimated')),
    input_tokens          INTEGER,
    output_tokens         INTEGER,
    reasoning_tokens      INTEGER,
    cache_read_tokens     INTEGER,
    cache_write_tokens    INTEGER,
    total_tokens          INTEGER,
    client_total_tokens   INTEGER,
    client_total_mismatch INTEGER NOT NULL DEFAULT 0 CHECK (client_total_mismatch IN (0, 1)),
    unknown_relations     TEXT,
    is_sidechain          INTEGER NOT NULL DEFAULT 0 CHECK (is_sidechain IN (0, 1)),
    source_file_id        INTEGER REFERENCES source_files (id) ON DELETE SET NULL,
    source_ref            TEXT,
    cost_source           TEXT    NOT NULL CHECK (cost_source IN ('client_reported', 'none')),
    client_cost_usd       REAL,
    imported_at_ms        INTEGER NOT NULL,
    updated_at_ms         INTEGER NOT NULL,
    UNIQUE (client, session_id, dedup_key)
);

CREATE INDEX idx_messages_session_created ON messages (session_id, created_at_ms);
CREATE INDEX idx_messages_client_created ON messages (client, created_at_ms);
CREATE INDEX idx_messages_model ON messages (model);
