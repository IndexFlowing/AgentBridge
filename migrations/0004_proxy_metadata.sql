-- 0004_proxy_metadata.sql: per-proxy health-check target and last verification state.
--
-- `test_url` lets each proxy define its own connectivity probe target (empty =
-- built-in stable default). The verification columns record the outcome of the
-- last "连接验证" so the Web UI can surface status and latency without leaking
-- credentials.
ALTER TABLE proxies ADD COLUMN test_url TEXT NOT NULL DEFAULT '';
ALTER TABLE proxies ADD COLUMN last_verified_at DATETIME;
ALTER TABLE proxies ADD COLUMN last_verified_ok BOOLEAN;
ALTER TABLE proxies ADD COLUMN last_verified_latency_ms INTEGER;
