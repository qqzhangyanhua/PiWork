CREATE TABLE connector_agent_grants (
    connection_id TEXT NOT NULL REFERENCES connector_connections(id) ON DELETE CASCADE,
    agent_instance_id TEXT NOT NULL REFERENCES agent_instances(id) ON DELETE CASCADE,
    permissions_json TEXT NOT NULL CHECK (
        json_valid(permissions_json)
        AND CASE WHEN json_valid(permissions_json)
            THEN json_type(permissions_json) = 'array'
            ELSE 0
        END
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (connection_id, agent_instance_id)
);

CREATE INDEX idx_connector_agent_grants_agent
    ON connector_agent_grants(agent_instance_id, connection_id);
