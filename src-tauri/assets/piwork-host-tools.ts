// PiWork host tools extension for the bundled Pi coding agent.
//
// This file is a thin, role-scoped transport: it registers exactly the host
// tools the Run's lease allows and forwards each call to the loopback Host
// Tool Bridge over HTTP. It contains no business logic, never touches SQLite
// or the workspace, and only trusts the four environment variables injected by
// the Engine Harness. The bridge re-validates every tool name and argument
// against the authoritative Rust schema, so the permissive schemas here are
// deliberate.
//
// Contract (see src-tauri/src/collaboration/tool_bridge.rs and tool_server.rs):
//   - default-export a factory `(api) => void`
//   - api.registerTool({ name, description, parameters, execute })
//   - execute POSTs { runId, token, tool, arguments } to PIWORK_HOST_TOOL_ENDPOINT
//
// Parameters are plain JSON Schema objects (Pi accepts JSON Schema as well as
// TypeBox; see the isJsonSchemaObject branch in the agent tool runner).

type JsonSchema = Record<string, unknown>;

interface ToolDefinition {
  description: string;
  parameters: JsonSchema;
}

const object = (properties: Record<string, JsonSchema>, required: string[] = []): JsonSchema => ({
  type: "object",
  properties,
  required,
  additionalProperties: false,
});

const stringArray = { type: "array", items: { type: "string" } } as const;
const stringField = { type: "string" } as const;
const numberField = { type: "number" } as const;
const anyObject = {} as const;

const TOOLS: Record<string, ToolDefinition> = {
  list_work_members: {
    description: "List the members (agents) assembled for the current Work and their roles.",
    parameters: object({}),
  },
  inspect_capability_packs: {
    description: "Inspect one or more capability packs by stable id.",
    parameters: object({ ids: stringArray }, ["ids"]),
  },
  delegate_assignment: {
    description:
      "Persist and enqueue a single child Assignment for a Work member. Returns immediately with the accepted assignment; it does not wait for the member to finish. Only the Lead may delegate.",
    parameters: object(
      {
        assignedAgentId: stringField,
        capabilityPackId: stringField,
        title: stringField,
        instruction: stringField,
        contextManifest: anyObject,
        expectedResultSchema: anyObject,
        acceptanceCriteria: anyObject,
        permissionScope: anyObject,
        priority: numberField,
        maxAttempts: numberField,
      },
      ["assignedAgentId", "title", "instruction"],
    ),
  },
  get_assignment_status: {
    description: "Fetch the status of the current or explicitly listed Assignments.",
    parameters: object({ assignmentIds: stringArray }),
  },
  cancel_assignment: {
    description: "Cancel a child Assignment that is not yet terminal.",
    parameters: object({ assignmentId: stringField }, ["assignmentId"]),
  },
  request_assignment_retry: {
    description: "Request a retry of a failed child Assignment within its attempt budget.",
    parameters: object({ assignmentId: stringField }, ["assignmentId"]),
  },
  record_work_decision: {
    description: "Record a durable Work decision into the Work Ledger.",
    parameters: object({ summary: stringField }, ["summary"]),
  },
  update_work_plan: {
    description: "Replace the Work plan in the Work Ledger with a new revision.",
    parameters: object({ plan: { type: "array", items: anyObject } }, ["plan"]),
  },
  complete_work_delivery: {
    description:
      "Submit the final Work delivery. Only the Lead may complete delivery, and required child Assignments must already be terminal.",
    parameters: object(
      {
        summary: stringField,
        artifacts: stringArray,
        validation: stringArray,
        limitations: stringArray,
      },
      ["summary"],
    ),
  },
  submit_assignment_result: {
    description:
      "Submit a structured Result Envelope for the current Member Assignment. Members must submit a result to finish their Assignment.",
    parameters: object({ envelope: anyObject }, ["envelope"]),
  },
  request_clarification: {
    description:
      "Ask the Lead or the user a clarifying question and put this Assignment into waiting.",
    parameters: object({ question: stringField, target: stringField }, ["question", "target"]),
  },
};

interface ExtensionApi {
  registerTool(tool: {
    name: string;
    description: string;
    parameters: JsonSchema;
    execute(
      toolCallId: string,
      params: Record<string, unknown>,
      signal: AbortSignal | undefined,
      onUpdate: unknown,
    ): Promise<{ content: string; details?: unknown; isError?: boolean }>;
  }): void;
}

export default async function activate(api: ExtensionApi): Promise<void> {
  const endpoint = process.env.PIWORK_HOST_TOOL_ENDPOINT;
  const token = process.env.PIWORK_HOST_TOOL_TOKEN;
  const runId = process.env.PIWORK_RUN_ID;
  const allowlist = (process.env.PIWORK_HOST_TOOLS ?? "")
    .split(",")
    .map((name) => name.trim())
    .filter((name) => name.length > 0);

  // Without a lease there is nothing to call; register nothing.
  if (!endpoint || !token || !runId) {
    return;
  }

  for (const name of allowlist) {
    const definition = TOOLS[name];
    if (!definition) {
      continue;
    }
    api.registerTool({
      name,
      description: definition.description,
      parameters: definition.parameters,
      async execute(_toolCallId, params, signal) {
        const response = await fetch(endpoint, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ runId, token, tool: name, arguments: params }),
          signal,
        });
        const payload: unknown = await response.json().catch(() => ({}));
        if (!response.ok) {
          const message =
            typeof payload === "object" && payload !== null && "error" in payload
              ? String((payload as { error: unknown }).error)
              : `host tool "${name}" failed with status ${response.status}`;
          return { content: message, isError: true };
        }
        return {
          content: JSON.stringify(payload),
          isError: false,
        };
      },
    });
  }
}
