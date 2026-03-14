CREATE TYPE crawl_status AS ENUM ('pending', 'processing', 'completed', 'failed');

CREATE TABLE frontier (
    url_hash BYTEA PRIMARY KEY,
    url TEXT NOT NULL,
    domain TEXT NOT NULL,
    priority INT NOT NULL DEFAULT 0,
    status crawl_status NOT NULL DEFAULT 'pending',
    available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    depth INT NOT NULL DEFAULT 0
);

CREATE INDEX idx_frontier_status_available_priority ON frontier (status, available_at, priority DESC);
CREATE INDEX idx_frontier_domain ON frontier (domain);

CREATE TABLE leads (
    fingerprint BYTEA PRIMARY KEY,
    full_name TEXT NOT NULL,
    contact_info JSONB NOT NULL DEFAULT '{}',
    score INT NOT NULL DEFAULT 0,
    signals JSONB NOT NULL DEFAULT '[]',
    source_url TEXT NOT NULL,
    discovered_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE domain_metrics (
    domain TEXT PRIMARY KEY,
    last_fetch_at TIMESTAMPTZ,
    crawl_delay_ms INT NOT NULL DEFAULT 1000,
    error_count INT NOT NULL DEFAULT 0
);
