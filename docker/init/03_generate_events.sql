-- poshgre test environment: generate ~1 million rows in analytics.events
-- Uses a doubling technique (INSERT ... SELECT FROM self) to avoid
-- writing millions of literal INSERT rows in this file.

-- ---- Step 1: Seed 1024 distinct base rows -------------------------
-- 4^5 = 1024 rows via generate_series cross join.
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT
    CASE (a + b + c + d + e) % 6
        WHEN 0 THEN 'page_view'
        WHEN 1 THEN 'click'
        WHEN 2 THEN 'scroll'
        WHEN 3 THEN 'form_submit'
        WHEN 4 THEN 'purchase'
        ELSE        'search'
    END AS event_type,
    1 + (a * 251 + b * 103 + c * 67 + d * 41 + e * 17) % 10000 AS user_id,
    CASE (a + b * 2 + c) % 10
        WHEN 0 THEN '/home'
        WHEN 1 THEN '/products'
        WHEN 2 THEN '/product/detail'
        WHEN 3 THEN '/cart'
        WHEN 4 THEN '/checkout'
        WHEN 5 THEN '/account'
        WHEN 6 THEN '/search'
        WHEN 7 THEN '/blog'
        WHEN 8 THEN '/about'
        ELSE        '/contact'
    END AS page_url,
    CASE (a + d) % 5
        WHEN 0 THEN 'https://google.com'
        WHEN 1 THEN 'https://twitter.com'
        WHEN 2 THEN NULL
        WHEN 3 THEN 'https://bing.com'
        ELSE        'https://facebook.com'
    END AS referrer,
    (10 + a)::text || '.' || (20 + b)::text || '.' || (30 + c)::text || '.' || (40 + d)::text AS ip_address,
    CASE (a + b + e) % 5
        WHEN 0 THEN 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0'
        WHEN 1 THEN 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) Safari/605.1'
        WHEN 2 THEN 'Mozilla/5.0 (X11; Linux x86_64) Firefox/121.0'
        WHEN 3 THEN 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0) Mobile/15E148'
        ELSE        'Mozilla/5.0 (Android 14; Mobile) Chrome/120.0'
    END AS user_agent,
    '2023-01-01 00:00:00'::timestamp
        + ((a * 86400 + b * 3600 + c * 600 + d * 60 + e * 10) || ' seconds')::interval AS created_at
FROM
    generate_series(0, 3) a,
    generate_series(0, 3) b,
    generate_series(0, 3) c,
    generate_series(0, 3) d,
    generate_series(0, 3) e;

-- ---- Step 2: Double the table repeatedly -------------------------
-- Each INSERT...SELECT doubles the row count.
-- After 10 doublings: 1024 * 2^10 = 1,048,576 rows (~1M).

-- Round 1: 1024 -> 2048
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '100 days'
FROM analytics.events;

-- Round 2: 2048 -> 4096
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '200 days'
FROM analytics.events;

-- Round 3: 4096 -> 8192
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '400 days'
FROM analytics.events;

-- Round 4: 8192 -> 16384
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '800 days'
FROM analytics.events;

-- Round 5: 16384 -> 32768
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '1600 days'
FROM analytics.events;

-- Round 6: 32768 -> 65536
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '3200 days'
FROM analytics.events;

-- Round 7: 65536 -> 131072
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '6400 days'
FROM analytics.events;

-- Round 8: 131072 -> 262144
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '12800 days'
FROM analytics.events;

-- Round 9: 262144 -> 524288
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '25600 days'
FROM analytics.events;

-- Round 10: 524288 -> 1048576
INSERT INTO analytics.events (event_type, user_id, page_url, referrer, ip_address, user_agent, created_at)
SELECT event_type, user_id, page_url, referrer, ip_address, user_agent,
    created_at + INTERVAL '51200 days'
FROM analytics.events;
