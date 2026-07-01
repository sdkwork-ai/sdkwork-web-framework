CREATE TABLE IF NOT EXISTS web_control_node (
    node_id TEXT PRIMARY KEY NOT NULL,
    region TEXT NOT NULL DEFAULT 'default',
    base_url TEXT NOT NULL,
    environment TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'registered',
    last_heartbeat_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_web_control_node_environment
    ON web_control_node (environment);

CREATE INDEX IF NOT EXISTS idx_web_control_node_region
    ON web_control_node (region);
