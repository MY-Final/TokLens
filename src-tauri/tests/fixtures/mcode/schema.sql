-- Entirely synthetic ledger; unrelated private columns are deliberately malformed.
PRAGMA journal_mode=WAL;
PRAGMA cache_size=1;
CREATE TABLE local_runtime_sessions (
 session_id TEXT PRIMARY KEY, parent_session_id TEXT, workspace_dir TEXT,
 created_at_ms INTEGER, updated_at_ms INTEGER, agent_name TEXT, runtime TEXT,
 record_json TEXT, title TEXT, history_relative_dir TEXT, future_private TEXT
);
CREATE TABLE local_runtime_token_usage (
 id INTEGER PRIMARY KEY, session_id TEXT, agent_name TEXT, framework_type TEXT,
 turn_id TEXT, model TEXT, ts INTEGER, input_tokens INTEGER, output_tokens INTEGER,
 reasoning_tokens INTEGER, cache_read_tokens INTEGER, cache_write_tokens INTEGER,
 cost_usd REAL, raw TEXT, future_private BLOB
);
CREATE TABLE unrelated_credentials (payload TEXT);
INSERT INTO unrelated_credentials VALUES ('PRIVATE_SENTINEL');
INSERT INTO local_runtime_sessions VALUES
 ('session-a',NULL,'/synthetic',1791763200000,1791849600000,'main','pi','{','PRIVATE_SENTINEL','versioned-history','PRIVATE_SENTINEL'),
 ('session-empty',NULL,NULL,1791763200000,1791849600000,'main','pi','{','PRIVATE_SENTINEL',NULL,NULL);
INSERT INTO local_runtime_token_usage VALUES
 (1,'session-a','main','pi-agent','shared-turn',NULL,1791849599999,10,2,0,100,7,99,'PRIVATE_SENTINEL',NULL),
 (2,'session-a','main','pi-agent','shared-turn','synthetic-model',1791849600000,20,3,0,200,0,99,'{',NULL);
