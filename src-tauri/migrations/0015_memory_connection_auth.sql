ALTER TABLE memory_connection_settings
ADD COLUMN hub_endpoint TEXT NOT NULL DEFAULT '';

ALTER TABLE memory_connection_settings
ADD COLUMN auth_mode TEXT NOT NULL DEFAULT 'gatewayBearer'
CHECK (auth_mode IN ('gatewayBearer', 'basic'));

ALTER TABLE memory_connection_settings
ADD COLUMN auth_username TEXT NOT NULL DEFAULT '';

ALTER TABLE memory_connection_settings
ADD COLUMN allow_insecure_http INTEGER NOT NULL DEFAULT 0
CHECK (allow_insecure_http IN (0, 1));

UPDATE memory_connection_settings
SET hub_endpoint = CASE
        WHEN hub_endpoint = '' THEN 'http://124.221.254.61'
        ELSE hub_endpoint
    END,
    auth_mode = CASE
        WHEN endpoint = '' THEN 'basic'
        ELSE auth_mode
    END,
    auth_username = CASE
        WHEN endpoint = '' THEN 'tdai'
        ELSE auth_username
    END,
    endpoint = CASE
        WHEN endpoint = '' THEN 'http://124.221.254.61/mem'
        ELSE endpoint
    END,
    service_id = CASE
        WHEN service_id = '' THEN 'default'
        ELSE service_id
    END,
    team_id = CASE
        WHEN team_id = '' THEN 'team-eb16plgnne'
        ELSE team_id
    END,
    user_id = CASE
        WHEN user_id = '' OR user_id = 'codo-local-user' THEN 'usr-1faz3ley78'
        ELSE user_id
    END,
    updated_at = CURRENT_TIMESTAMP
WHERE singleton_id = 1;
