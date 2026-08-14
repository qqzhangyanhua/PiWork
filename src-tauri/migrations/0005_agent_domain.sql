CREATE TABLE role_templates (
    id TEXT PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL,
    role_kind TEXT NOT NULL CHECK (role_kind IN ('lead', 'researcher', 'engineer', 'reviewer')),
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    base_instructions TEXT NOT NULL,
    responsibilities_json TEXT NOT NULL CHECK (json_valid(responsibilities_json) AND CASE WHEN json_valid(responsibilities_json) THEN json_type(responsibilities_json) = 'array' ELSE 0 END),
    non_responsibilities_json TEXT NOT NULL CHECK (json_valid(non_responsibilities_json) AND CASE WHEN json_valid(non_responsibilities_json) THEN json_type(non_responsibilities_json) = 'array' ELSE 0 END),
    base_result_contract_json TEXT NOT NULL CHECK (json_valid(base_result_contract_json) AND CASE WHEN json_valid(base_result_contract_json) THEN json_type(base_result_contract_json) = 'object' ELSE 0 END),
    compatible_capability_kinds_json TEXT NOT NULL CHECK (json_valid(compatible_capability_kinds_json) AND CASE WHEN json_valid(compatible_capability_kinds_json) THEN json_type(compatible_capability_kinds_json) = 'array' ELSE 0 END),
    builtin INTEGER NOT NULL CHECK (builtin IN (0, 1)),
    version INTEGER NOT NULL CHECK (version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (slug, version)
);

CREATE TABLE agent_definitions (
    id TEXT PRIMARY KEY NOT NULL,
    role_template_id TEXT NOT NULL REFERENCES role_templates(id) ON DELETE RESTRICT,
    slug TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    instructions TEXT NOT NULL,
    responsibilities_json TEXT NOT NULL CHECK (json_valid(responsibilities_json) AND CASE WHEN json_valid(responsibilities_json) THEN json_type(responsibilities_json) = 'array' ELSE 0 END),
    non_responsibilities_json TEXT NOT NULL CHECK (json_valid(non_responsibilities_json) AND CASE WHEN json_valid(non_responsibilities_json) THEN json_type(non_responsibilities_json) = 'array' ELSE 0 END),
    input_contract_json TEXT NOT NULL CHECK (json_valid(input_contract_json) AND CASE WHEN json_valid(input_contract_json) THEN json_type(input_contract_json) = 'object' ELSE 0 END),
    result_contract_json TEXT NOT NULL CHECK (json_valid(result_contract_json) AND CASE WHEN json_valid(result_contract_json) THEN json_type(result_contract_json) = 'object' ELSE 0 END),
    quality_rubric_json TEXT NOT NULL CHECK (json_valid(quality_rubric_json) AND CASE WHEN json_valid(quality_rubric_json) THEN json_type(quality_rubric_json) = 'object' ELSE 0 END),
    default_engine_kind TEXT NOT NULL,
    default_model_configuration_id TEXT,
    default_permission_policy TEXT NOT NULL CHECK (default_permission_policy IN ('inherit_work', 'read_only', 'work_write')),
    default_parallelism INTEGER NOT NULL CHECK (default_parallelism BETWEEN 1 AND 8),
    memory_policy TEXT NOT NULL CHECK (memory_policy = 'confirmed_only'),
    builtin INTEGER NOT NULL CHECK (builtin IN (0, 1)),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    version INTEGER NOT NULL CHECK (version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (slug, version)
);

CREATE INDEX idx_agent_definitions_role_template_id ON agent_definitions(role_template_id);

CREATE TABLE agent_instances (
    id TEXT PRIMARY KEY NOT NULL,
    definition_id TEXT NOT NULL REFERENCES agent_definitions(id) ON DELETE RESTRICT,
    display_name TEXT NOT NULL,
    engine_override TEXT,
    model_configuration_override TEXT,
    permission_policy_override TEXT CHECK (permission_policy_override IS NULL OR permission_policy_override IN ('inherit_work', 'read_only', 'work_write')),
    parallelism_override INTEGER CHECK (parallelism_override IS NULL OR parallelism_override BETWEEN 1 AND 8),
    builtin INTEGER NOT NULL CHECK (builtin IN (0, 1)),
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_agent_instances_definition_id ON agent_instances(definition_id);

CREATE TABLE capability_packs (
    id TEXT PRIMARY KEY NOT NULL,
    catalog_capability_id TEXT UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    instructions TEXT NOT NULL,
    input_schema_json TEXT NOT NULL CHECK (json_valid(input_schema_json) AND CASE WHEN json_valid(input_schema_json) THEN json_type(input_schema_json) = 'object' ELSE 0 END),
    output_schema_json TEXT NOT NULL CHECK (json_valid(output_schema_json) AND CASE WHEN json_valid(output_schema_json) THEN json_type(output_schema_json) = 'object' ELSE 0 END),
    procedure_json TEXT NOT NULL CHECK (json_valid(procedure_json) AND CASE WHEN json_valid(procedure_json) THEN json_type(procedure_json) = 'object' ELSE 0 END),
    validation_rubric_json TEXT NOT NULL CHECK (json_valid(validation_rubric_json) AND CASE WHEN json_valid(validation_rubric_json) THEN json_type(validation_rubric_json) = 'object' ELSE 0 END),
    required_tools_json TEXT NOT NULL CHECK (json_valid(required_tools_json) AND CASE WHEN json_valid(required_tools_json) THEN json_type(required_tools_json) = 'array' ELSE 0 END),
    default_permission_scope TEXT NOT NULL CHECK (default_permission_scope IN ('inherit_work', 'read_only', 'work_write')),
    compatible_role_template_ids_json TEXT NOT NULL CHECK (json_valid(compatible_role_template_ids_json) AND CASE WHEN json_valid(compatible_role_template_ids_json) THEN json_type(compatible_role_template_ids_json) = 'array' ELSE 0 END),
    required_engine_capabilities_json TEXT NOT NULL CHECK (json_valid(required_engine_capabilities_json) AND CASE WHEN json_valid(required_engine_capabilities_json) THEN json_type(required_engine_capabilities_json) = 'array' ELSE 0 END),
    conflicts_with_capability_pack_ids_json TEXT NOT NULL CHECK (json_valid(conflicts_with_capability_pack_ids_json) AND CASE WHEN json_valid(conflicts_with_capability_pack_ids_json) THEN json_type(conflicts_with_capability_pack_ids_json) = 'array' ELSE 0 END),
    version INTEGER NOT NULL CHECK (version > 0),
    status TEXT NOT NULL CHECK (status IN ('catalog_only', 'executable', 'deprecated')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_capability_packs_status ON capability_packs(status, name);

CREATE TRIGGER role_templates_string_arrays_insert
BEFORE INSERT ON role_templates
WHEN EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.responsibilities_json)
            THEN CASE WHEN json_type(NEW.responsibilities_json) = 'array'
                THEN NEW.responsibilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.non_responsibilities_json)
            THEN CASE WHEN json_type(NEW.non_responsibilities_json) = 'array'
                THEN NEW.non_responsibilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.compatible_capability_kinds_json)
            THEN CASE WHEN json_type(NEW.compatible_capability_kinds_json) = 'array'
                THEN NEW.compatible_capability_kinds_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
)
BEGIN
    SELECT RAISE(ABORT, 'JSON string array contains non-text element');
END;

CREATE TRIGGER role_templates_string_arrays_update
BEFORE UPDATE OF responsibilities_json, non_responsibilities_json, compatible_capability_kinds_json
ON role_templates
WHEN EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.responsibilities_json)
            THEN CASE WHEN json_type(NEW.responsibilities_json) = 'array'
                THEN NEW.responsibilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.non_responsibilities_json)
            THEN CASE WHEN json_type(NEW.non_responsibilities_json) = 'array'
                THEN NEW.non_responsibilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.compatible_capability_kinds_json)
            THEN CASE WHEN json_type(NEW.compatible_capability_kinds_json) = 'array'
                THEN NEW.compatible_capability_kinds_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
)
BEGIN
    SELECT RAISE(ABORT, 'JSON string array contains non-text element');
END;

CREATE TRIGGER agent_definitions_string_arrays_insert
BEFORE INSERT ON agent_definitions
WHEN EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.responsibilities_json)
            THEN CASE WHEN json_type(NEW.responsibilities_json) = 'array'
                THEN NEW.responsibilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.non_responsibilities_json)
            THEN CASE WHEN json_type(NEW.non_responsibilities_json) = 'array'
                THEN NEW.non_responsibilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
)
BEGIN
    SELECT RAISE(ABORT, 'JSON string array contains non-text element');
END;

CREATE TRIGGER agent_definitions_string_arrays_update
BEFORE UPDATE OF responsibilities_json, non_responsibilities_json ON agent_definitions
WHEN EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.responsibilities_json)
            THEN CASE WHEN json_type(NEW.responsibilities_json) = 'array'
                THEN NEW.responsibilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.non_responsibilities_json)
            THEN CASE WHEN json_type(NEW.non_responsibilities_json) = 'array'
                THEN NEW.non_responsibilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
)
BEGIN
    SELECT RAISE(ABORT, 'JSON string array contains non-text element');
END;

CREATE TRIGGER capability_packs_string_arrays_insert
BEFORE INSERT ON capability_packs
WHEN EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.required_tools_json)
            THEN CASE WHEN json_type(NEW.required_tools_json) = 'array'
                THEN NEW.required_tools_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.compatible_role_template_ids_json)
            THEN CASE WHEN json_type(NEW.compatible_role_template_ids_json) = 'array'
                THEN NEW.compatible_role_template_ids_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.required_engine_capabilities_json)
            THEN CASE WHEN json_type(NEW.required_engine_capabilities_json) = 'array'
                THEN NEW.required_engine_capabilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.conflicts_with_capability_pack_ids_json)
            THEN CASE WHEN json_type(NEW.conflicts_with_capability_pack_ids_json) = 'array'
                THEN NEW.conflicts_with_capability_pack_ids_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
)
BEGIN
    SELECT RAISE(ABORT, 'JSON string array contains non-text element');
END;

CREATE TRIGGER capability_packs_string_arrays_update
BEFORE UPDATE OF required_tools_json, compatible_role_template_ids_json,
    required_engine_capabilities_json, conflicts_with_capability_pack_ids_json
ON capability_packs
WHEN EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.required_tools_json)
            THEN CASE WHEN json_type(NEW.required_tools_json) = 'array'
                THEN NEW.required_tools_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.compatible_role_template_ids_json)
            THEN CASE WHEN json_type(NEW.compatible_role_template_ids_json) = 'array'
                THEN NEW.compatible_role_template_ids_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.required_engine_capabilities_json)
            THEN CASE WHEN json_type(NEW.required_engine_capabilities_json) = 'array'
                THEN NEW.required_engine_capabilities_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
) OR EXISTS (
    SELECT 1 FROM json_each(
        CASE WHEN json_valid(NEW.conflicts_with_capability_pack_ids_json)
            THEN CASE WHEN json_type(NEW.conflicts_with_capability_pack_ids_json) = 'array'
                THEN NEW.conflicts_with_capability_pack_ids_json ELSE '[]' END
            ELSE '[]' END
    ) WHERE type <> 'text'
)
BEGIN
    SELECT RAISE(ABORT, 'JSON string array contains non-text element');
END;

CREATE TABLE agent_capability_bindings (
    agent_definition_id TEXT NOT NULL REFERENCES agent_definitions(id) ON DELETE CASCADE,
    capability_pack_id TEXT NOT NULL REFERENCES capability_packs(id) ON DELETE RESTRICT,
    installed_at TEXT NOT NULL,
    PRIMARY KEY (agent_definition_id, capability_pack_id)
);

CREATE INDEX idx_agent_capability_bindings_pack_id ON agent_capability_bindings(capability_pack_id);

CREATE TABLE work_agents (
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    agent_instance_id TEXT NOT NULL REFERENCES agent_instances(id) ON DELETE RESTRICT,
    role_kind TEXT NOT NULL CHECK (role_kind IN ('lead', 'researcher', 'engineer', 'reviewer')),
    status TEXT NOT NULL CHECK (status IN ('joined', 'inactive')),
    permission_policy TEXT NOT NULL CHECK (permission_policy IN ('inherit_work', 'read_only', 'work_write')),
    joined_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (work_id, agent_instance_id)
);

CREATE INDEX idx_work_agents_instance_id ON work_agents(agent_instance_id);
CREATE INDEX idx_work_agents_work_role ON work_agents(work_id, role_kind, status);

CREATE TABLE work_leads (
    work_id TEXT PRIMARY KEY NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    agent_instance_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (work_id, agent_instance_id)
        REFERENCES work_agents(work_id, agent_instance_id) ON DELETE NO ACTION
);

CREATE INDEX idx_work_leads_instance_id ON work_leads(agent_instance_id);

CREATE TRIGGER work_leads_require_joined_lead_insert
BEFORE INSERT ON work_leads
WHEN NOT EXISTS (
    SELECT 1 FROM work_agents
    WHERE work_id = NEW.work_id
      AND agent_instance_id = NEW.agent_instance_id
      AND role_kind = 'lead'
      AND status = 'joined'
)
BEGIN
    SELECT RAISE(ABORT, 'work lead must be joined lead');
END;

CREATE TRIGGER work_leads_require_joined_lead_update
BEFORE UPDATE OF work_id, agent_instance_id ON work_leads
WHEN NOT EXISTS (
    SELECT 1 FROM work_agents
    WHERE work_id = NEW.work_id
      AND agent_instance_id = NEW.agent_instance_id
      AND role_kind = 'lead'
      AND status = 'joined'
)
BEGIN
    SELECT RAISE(ABORT, 'work lead must be joined lead');
END;

CREATE TRIGGER work_agents_preserve_current_lead
BEFORE UPDATE OF role_kind, status ON work_agents
WHEN (NEW.role_kind <> 'lead' OR NEW.status <> 'joined')
 AND EXISTS (
    SELECT 1 FROM work_leads
    WHERE work_id = OLD.work_id
      AND agent_instance_id = OLD.agent_instance_id
 )
BEGIN
    SELECT RAISE(ABORT, 'current work lead must remain joined lead');
END;

INSERT INTO role_templates (
    id, slug, role_kind, name, description, base_instructions,
    responsibilities_json, non_responsibilities_json, base_result_contract_json,
    compatible_capability_kinds_json, builtin, version, created_at, updated_at
) VALUES
    ('role-template:lead:v1', 'lead', 'lead', 'PiWork 主理人', '理解目标并统筹团队交付。', '理解工作目标，拆解任务，分派合适成员，作出取舍，综合证据并交付最终结果。', '["understand","decompose","dispatch","decide","synthesize","deliver"]', '["不伪造成员结论","不绕过权限边界"]', '{"summary":"string","decisions":"array","deliverables":"array","open_risks":"array"}', '[]', 1, 1, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('role-template:researcher:v1', 'researcher', 'researcher', '研究员', '基于来源开展研究与核验。', '检索来源证据，核查事实，比较方案，识别风险并明确置信度。', '["source_evidence","fact_check","comparison","risk","confidence"]', '["不修改工作文件","不把推测表述为事实"]', '{"findings":"array","sources":"array","risks":"array","confidence":"string"}', '[]', 1, 1, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('role-template:engineer:v1', 'engineer', 'engineer', '工程师', '实现、调试并验证工程产出。', '实现需求，调试问题，重构代码，运行测试并产出可复核制品。', '["implement","debug","refactor","test","artifacts"]', '["不扩大任务范围","不隐瞒未验证结果"]', '{"changes":"array","tests":"array","artifacts":"array","risks":"array"}', '[]', 1, 1, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('role-template:reviewer:v1', 'reviewer', 'reviewer', '审阅者', '独立审查主张与变更。', '独立审查主张、差异、测试、权限边界和遗漏，并报告可操作问题。', '["claims_review","diff_review","tests_review","permission_review","omission_review"]', '["不替代实现者修改产出","不在证据不足时宣称通过"]', '{"findings":"array","evidence":"array","verdict":"string"}', '[]', 1, 1, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z');

INSERT INTO agent_definitions (
    id, role_template_id, slug, name, description, instructions,
    responsibilities_json, non_responsibilities_json, input_contract_json,
    result_contract_json, quality_rubric_json, default_engine_kind,
    default_model_configuration_id, default_permission_policy, default_parallelism,
    memory_policy, builtin, active, version, created_at, updated_at
) VALUES
    ('agent-definition:piwork-lead:v1', 'role-template:lead:v1', 'piwork-lead', 'PiWork 主理人', 'PiWork 内置团队主理人。', '先理解目标与约束，再拆解和分派；综合成员证据，明确决策、风险和最终交付。', '["understand","decompose","dispatch","decide","synthesize","deliver"]', '["不伪造成员结论","不绕过权限边界"]', '{"goal":"string","context":"object","constraints":"array"}', '{"summary":"string","decisions":"array","deliverables":"array","open_risks":"array"}', '{"goal_coverage":true,"evidence_traceability":true,"risk_disclosure":true}', 'pi', NULL, 'inherit_work', 1, 'confirmed_only', 1, 1, 1, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('agent-definition:piwork-researcher:v1', 'role-template:researcher:v1', 'piwork-researcher', '研究员', 'PiWork 内置研究员。', '使用可信来源取证，区分事实与推断，比较备选项并报告风险和置信度。', '["source_evidence","fact_check","comparison","risk","confidence"]', '["不修改工作文件","不把推测表述为事实"]', '{"question":"string","scope":"object","source_constraints":"array"}', '{"findings":"array","sources":"array","risks":"array","confidence":"string"}', '{"source_quality":true,"claim_support":true,"uncertainty_disclosed":true}', 'pi', NULL, 'read_only', 1, 'confirmed_only', 1, 1, 1, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('agent-definition:piwork-engineer:v1', 'role-template:engineer:v1', 'piwork-engineer', '工程师', 'PiWork 内置工程师。', '在授权范围内实现、调试和重构；运行相关测试并提供可复核产出与未解决风险。', '["implement","debug","refactor","test","artifacts"]', '["不扩大任务范围","不隐瞒未验证结果"]', '{"requirements":"array","workspace":"string","constraints":"array"}', '{"changes":"array","tests":"array","artifacts":"array","risks":"array"}', '{"requirements_met":true,"tests_reported":true,"scope_respected":true}', 'pi', NULL, 'inherit_work', 1, 'confirmed_only', 1, 1, 1, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('agent-definition:piwork-reviewer:v1', 'role-template:reviewer:v1', 'piwork-reviewer', '审阅者', 'PiWork 内置独立审阅者。', '独立核查主张、变更和测试，审视权限与遗漏，以证据和优先级报告问题。', '["claims_review","diff_review","tests_review","permission_review","omission_review"]', '["不替代实现者修改产出","不在证据不足时宣称通过"]', '{"claims":"array","diff":"object","test_evidence":"array"}', '{"findings":"array","evidence":"array","verdict":"string"}', '{"independence":true,"actionable_findings":true,"evidence_based":true}', 'pi', NULL, 'read_only', 1, 'confirmed_only', 1, 1, 1, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z');

INSERT INTO agent_instances (
    id, definition_id, display_name, engine_override, model_configuration_override,
    permission_policy_override, parallelism_override, builtin, status, created_at, updated_at
) VALUES
    ('agent-instance:piwork-lead', 'agent-definition:piwork-lead:v1', 'PiWork 主理人', NULL, NULL, NULL, NULL, 1, 'active', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('agent-instance:piwork-researcher', 'agent-definition:piwork-researcher:v1', '研究员', NULL, NULL, NULL, NULL, 1, 'active', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('agent-instance:piwork-engineer', 'agent-definition:piwork-engineer:v1', '工程师', NULL, NULL, NULL, NULL, 1, 'active', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('agent-instance:piwork-reviewer', 'agent-definition:piwork-reviewer:v1', '审阅者', NULL, NULL, NULL, NULL, 1, 'active', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z');

INSERT INTO capability_packs (
    id, catalog_capability_id, name, description, instructions,
    input_schema_json, output_schema_json, procedure_json, validation_rubric_json,
    required_tools_json, default_permission_scope, compatible_role_template_ids_json,
    required_engine_capabilities_json, conflicts_with_capability_pack_ids_json,
    version, status, created_at, updated_at
) VALUES
    ('capability-pack:lead-coordination:v1', NULL, '主理协调', '为主理人提供理解、拆解、分派、决策、综合和交付方法。', '建立目标与约束，形成可验证任务，选择成员并综合各方结果；保留证据、分歧和未决风险。', '{"goal":"string","context":"object","constraints":"array"}', '{"plan":"array","decisions":"array","delivery":"object"}', '{"steps":["understand","decompose","dispatch","decide","synthesize","deliver"]}', '{"checks":["goal_covered","assignments_clear","evidence_preserved","risks_disclosed"]}', '["read","grep","find","ls"]', 'inherit_work', '["role-template:lead:v1"]', '[]', '[]', 1, 'executable', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('capability-pack:source-research:v1', NULL, '来源研究', '为研究员提供来源检索、事实核查、比较和风险评估方法。', '优先使用原始可信来源；逐项关联主张与证据，标记冲突、时效和置信度。', '{"question":"string","source_constraints":"array"}', '{"claims":"array","sources":"array","confidence":"string"}', '{"steps":["scope","retrieve","verify","compare","report"]}', '{"checks":["sources_cited","claims_supported","uncertainty_disclosed"]}', '["read","grep","find","ls"]', 'read_only', '["role-template:researcher:v1"]', '[]', '[]', 1, 'executable', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('capability-pack:engineering-execution:v1', NULL, '工程执行', '为工程师提供实现、调试、重构、测试和制品交付方法。', '先确认需求和边界，实施最小充分变更，运行相关验证并如实报告结果与风险。', '{"requirements":"array","workspace":"string","constraints":"array"}', '{"changes":"array","tests":"array","artifacts":"array"}', '{"steps":["inspect","implement","test","refactor","report"]}', '{"checks":["requirements_met","tests_passed","scope_respected"]}', '["read","grep","find","ls","edit","write","bash"]', 'inherit_work', '["role-template:engineer:v1"]', '[]', '[]', 1, 'executable', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'),
    ('capability-pack:independent-review:v1', NULL, '独立审阅', '为审阅者提供独立主张、差异、测试、权限和遗漏审查方法。', '从要求和证据独立复核产出；按影响和证据报告问题，不修改被审阅产出。', '{"requirements":"array","claims":"array","diff":"object","tests":"array"}', '{"findings":"array","verdict":"string"}', '{"steps":["establish_contract","inspect_evidence","challenge_claims","report_findings"]}', '{"checks":["independent","evidence_based","permissions_checked","omissions_checked"]}', '["read","grep","find","ls"]', 'read_only', '["role-template:reviewer:v1"]', '[]', '[]', 1, 'executable', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z');

INSERT INTO capability_packs (
    id, catalog_capability_id, name, description, instructions,
    input_schema_json, output_schema_json, procedure_json, validation_rubric_json,
    required_tools_json, default_permission_scope, compatible_role_template_ids_json,
    required_engine_capabilities_json, conflicts_with_capability_pack_ids_json,
    version, status, created_at, updated_at
)
SELECT
    printf('catalog-capability:%03d', catalog.column1),
    printf('catalog-capability:%03d', catalog.column1),
    catalog.column2, '', '', '{}', '{}', '{}', '{}', '[]', 'read_only', '[]', '[]', '[]',
    1, 'catalog_only', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'
FROM (VALUES
    (1, '智能任务分流智能体'), (2, '多模态资料接收智能体'), (3, '创新想法理解智能体'),
    (4, '概念方案生成智能体'), (5, '市场机会研究智能体'), (6, '知识产权检索智能体'),
    (7, '政策与资金匹配智能体'), (8, '生态服务推荐智能体'), (9, '线索分析智能体'),
    (10, '客户主体核验智能体'), (11, '客户画像智能体'), (12, '商机资格判断智能体'),
    (13, '销售跟进策略智能体'), (14, '会议与沟通纪要智能体'), (15, 'NDA与资料准备智能体'),
    (16, '需求澄清智能体'), (17, '产品事实管理智能体'), (18, '需求冲突与变更识别智能体'),
    (19, '服务拆解智能体'), (20, '评估工程师匹配智能体'), (21, '相似项目检索智能体'),
    (22, '技术方案检索智能体'), (23, '技术可行性评估智能体'), (24, '行业标准与合规智能体'),
    (25, '研发人天估算智能体'), (26, 'BOM与样机成本智能体'), (27, '外部询价与价格比较智能体'),
    (28, '参考报价智能体'), (29, '定价与毛利策略智能体'), (30, '报价预审与版本智能体'),
    (31, '报价解释与谈判辅助智能体'), (32, '合同生成智能体'), (33, '合同一致性与风险智能体'),
    (34, '收款与支付助手'), (35, '对账与结算智能体'), (36, '发票与税务辅助智能体'),
    (37, '项目拆解智能体'), (38, '项目组组建智能体'), (39, '计划排程智能体'),
    (40, 'AI项目助理'), (41, '任务分派与自动化智能体'), (42, '会议行动跟踪智能体'),
    (43, '项目风险预警智能体'), (44, '需求变更影响智能体'), (45, '执行前复核智能体'),
    (46, '产出物与验收管理智能体'), (47, '产品系统架构智能体'), (48, '工业设计与概念图智能体'),
    (49, '机械结构方案智能体'), (50, '电子系统方案智能体'), (51, '器件与关键物料选型智能体'),
    (52, '原理图审查智能体'), (53, 'PCB与DFM审查智能体'), (54, '固件与软件开发智能体'),
    (55, '工程BOM智能体'), (56, '模块复用智能体'), (57, '设计评审智能体'),
    (58, '测试方案智能体'), (59, '工程文档生成智能体'), (60, '跨专业一致性智能体'),
    (61, '工厂与供应商匹配智能体'), (62, 'RFQ生成与询价智能体'), (63, '报价归一与比价智能体'),
    (64, '供应商评分智能体'), (65, '采购审批建议智能体'), (66, '物料齐套与替代智能体'),
    (67, '样机试制规划智能体'), (68, '工艺路线智能体'), (69, '作业指导智能体'),
    (70, '生产排程智能体'), (71, '生产任务助手'), (72, '语音报工与记录智能体'),
    (73, '质量检验智能体'), (74, '缺陷与根因分析智能体'), (75, '设备维护智能体'),
    (76, '制造成本复盘智能体'), (77, '客户验收智能体'), (78, '售后问题分诊智能体'),
    (79, '故障诊断智能体'), (80, '用户反馈与迭代智能体'), (81, '项目归档智能体'),
    (82, '知识提取智能体'), (83, '知识质量与复用智能体'), (84, '工艺包组装智能体'),
    (85, '总控编排智能体'), (86, '工作流配置智能体'), (87, '模型路由智能体'),
    (88, '知识检索编排智能体'), (89, '记忆与上下文智能体'), (90, '权限与数据安全智能体'),
    (91, '数据质量智能体'), (92, '智能体评测智能体'), (93, '证据与幻觉检查智能体'),
    (94, 'AI成本运营智能体'), (95, '审计与决策日志智能体'), (96, '反馈学习智能体')
) AS catalog;

INSERT INTO agent_capability_bindings (agent_definition_id, capability_pack_id, installed_at) VALUES
    ('agent-definition:piwork-lead:v1', 'capability-pack:lead-coordination:v1', '2026-08-14T00:00:00Z'),
    ('agent-definition:piwork-researcher:v1', 'capability-pack:source-research:v1', '2026-08-14T00:00:00Z'),
    ('agent-definition:piwork-engineer:v1', 'capability-pack:engineering-execution:v1', '2026-08-14T00:00:00Z'),
    ('agent-definition:piwork-reviewer:v1', 'capability-pack:independent-review:v1', '2026-08-14T00:00:00Z');

INSERT INTO work_agents (
    work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at
)
SELECT
    id, 'agent-instance:piwork-lead', 'lead', 'joined', 'inherit_work',
    created_at, created_at
FROM works;

INSERT INTO work_leads (work_id, agent_instance_id, created_at)
SELECT id, 'agent-instance:piwork-lead', created_at
FROM works;
