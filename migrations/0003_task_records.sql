-- 0003_task_records.sql: make task_id the stable task identity.
--
-- The legacy `tasks` table was keyed by `project_name`, so every project could
-- keep only its latest task and concurrent tasks overwrote each other. The new
-- `task_records` table keys every task by its globally unique `task_id`;
-- `project_name` is only an ownership/filter column.
CREATE TABLE IF NOT EXISTS task_records (
    task_id TEXT PRIMARY KEY,
    project_name TEXT NOT NULL,
    state_json TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_task_records_project
    ON task_records(project_name, updated_at DESC);

-- Backfill existing single-task-per-project rows without data loss.
INSERT OR IGNORE INTO task_records (task_id, project_name, state_json, updated_at)
SELECT json_extract(state_json, '$.task_id'),
       project_name,
       state_json,
       updated_at
FROM tasks
WHERE json_valid(state_json)
  AND json_extract(state_json, '$.task_id') IS NOT NULL
  AND json_extract(state_json, '$.task_id') <> '';
