BEGIN IMMEDIATE;
ALTER TABLE sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'unknown'
    CHECK(status IN ('unknown','incomplete','ended'));
ALTER TABLE sessions ADD COLUMN started_at INTEGER;
ALTER TABLE sessions ADD COLUMN ended_at INTEGER;
INSERT INTO migrations VALUES(2,'');
PRAGMA user_version=2;
COMMIT;
