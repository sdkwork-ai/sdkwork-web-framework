CREATE TABLE IF NOT EXISTS web_rate_limit_bucket (
    bucket_key TEXT PRIMARY KEY NOT NULL,
    request_count INTEGER NOT NULL,
    window_start INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_web_rate_limit_expires
    ON web_rate_limit_bucket (expires_at);
