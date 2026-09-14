-- 0002_skills.sql: Skill system metadata and project policy records
CREATE TABLE IF NOT EXISTS skills (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '1.0.0',
    source TEXT NOT NULL DEFAULT 'local',
    path TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    installed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS project_skills (
    project_name TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (project_name, skill_id)
);

CREATE INDEX IF NOT EXISTS idx_project_skills_project
    ON project_skills(project_name);