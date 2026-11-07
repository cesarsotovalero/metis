BEGIN IMMEDIATE;
CREATE TABLE event_metadata(
    event_id TEXT PRIMARY KEY REFERENCES events(event_id),
    kind TEXT NOT NULL,
    visibility TEXT NOT NULL CHECK(visibility='synthetic'),
    source_timestamp INTEGER
);
INSERT INTO event_metadata SELECT event_id,json_extract(payload_json,'$.payload.kind'),
    json_extract(payload_json,'$.visibility'),
    json_extract(payload_json,'$.source_timestamp_unix_ms') FROM events;
CREATE INDEX event_metadata_kind ON event_metadata(kind,event_id);
CREATE TABLE tasks(
    session_id TEXT NOT NULL REFERENCES sessions(id),
    id TEXT NOT NULL,
    outcome_json TEXT NOT NULL CHECK(json_valid(outcome_json)),
    oracle_version TEXT,
    evidence_kind TEXT CHECK(evidence_kind IN ('observed','agent_reported','inferred')),
    started_at INTEGER,
    ended_at INTEGER,
    PRIMARY KEY(session_id,id)
);
ALTER TABLE requests RENAME TO requests_v2;
CREATE TABLE requests(
    id TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES sessions(id),
    upstream_id TEXT, provider TEXT, model TEXT,
    status TEXT NOT NULL CHECK(status IN ('incomplete','streaming','completed','failed','cancelled','unknown')),
    normalized_usage_json TEXT CHECK(json_valid(normalized_usage_json)), task_id TEXT,
    PRIMARY KEY(session_id,id), FOREIGN KEY(session_id,task_id) REFERENCES tasks(session_id,id)
);
INSERT INTO requests(id,session_id,upstream_id,provider,model,status,normalized_usage_json)
    SELECT id,session_id,upstream_id,provider,model,status,normalized_usage_json FROM requests_v2;
DROP TABLE requests_v2;
ALTER TABLE tool_calls RENAME TO tool_calls_v2;
CREATE TABLE tool_calls(
    id TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES sessions(id), parent_request_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('incomplete','streaming','completed','failed','cancelled','unknown')),
    exit_code INTEGER, task_id TEXT,
    PRIMARY KEY(session_id,id), FOREIGN KEY(session_id,task_id) REFERENCES tasks(session_id,id)
);
INSERT INTO tool_calls(id,session_id,parent_request_id,status,exit_code)
    SELECT id,session_id,parent_request_id,status,exit_code FROM tool_calls_v2;
DROP TABLE tool_calls_v2;
CREATE TABLE session_epochs(
    session_id TEXT NOT NULL REFERENCES sessions(id),
    producer_id TEXT NOT NULL,
    epoch TEXT NOT NULL,
    previous_epoch TEXT,
    PRIMARY KEY(session_id,producer_id,epoch)
);
INSERT INTO migrations VALUES(3,'');
PRAGMA user_version=3;
COMMIT;
