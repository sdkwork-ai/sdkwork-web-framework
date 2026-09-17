CREATE TABLE IF NOT EXISTS framework_control_node (
    node_id TEXT PRIMARY KEY NOT NULL,
    region TEXT NOT NULL DEFAULT 'default',
    base_url TEXT NOT NULL,
    environment TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'registered',
    last_heartbeat_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_framework_control_node_environment
    ON framework_control_node (environment);

CREATE INDEX IF NOT EXISTS idx_framework_control_node_region
    ON framework_control_node (region);
