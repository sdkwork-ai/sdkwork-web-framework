CREATE TABLE IF NOT EXISTS framework_rate_limit_bucket (
    bucket_key TEXT PRIMARY KEY NOT NULL,
    request_count INTEGER NOT NULL,
    window_start INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_framework_rate_limit_expires
    ON framework_rate_limit_bucket (expires_at);
