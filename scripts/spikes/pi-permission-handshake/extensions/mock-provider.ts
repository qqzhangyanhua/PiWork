/**
 * Spike-only mock provider. Not loaded by production PiEngineAdapter.
 *
 * Registers a local streamSimple provider so the bundled sidecar can emit
 * read/edit/write/bash tool calls without live model credentials.
 *
 * SPIKE_TOOLS controls the sequence, comma-separated, e.g. "read,write,bash".
 */

type ContentBlock = {
  type: string;
  text?: string;
  id?: string;
  name?: string;
  arguments?: Record<string, unknown>;
};

type AssistantMessage = {
  role: "assistant";
  content: ContentBlock[];
  api: string;
  provider: string;
  model: string;
  usage: {
    input: number;
    output: number;
    cacheRead: number;
    cacheWrite: number;
    totalTokens: number;
    cost: { input: number; output: number; cacheRead: number; cacheWrite: number; total: number };
  };
  stopReason: string;
  timestamp: number;
};

type StreamEvent = Record<string, unknown>;

type Waiter = (result: { value?: StreamEvent; done: boolean }) => void;

function createAssistantStream() {
  const queue: StreamEvent[] = [];
  const waiting: Waiter[] = [];
  let done = false;
  let resolveFinal: (value: unknown) => void = () => {};
  const finalResultPromise = new Promise((resolve) => {
    resolveFinal = resolve;
  });

  return {
    push(event: StreamEvent) {
      if (done) {
        return;
      }
      if (event.type === "done" || event.type === "error") {
        done = true;
        resolveFinal(event.type === "done" ? event.message : event.error);
      }
      const waiter = waiting.shift();
      if (waiter) {
        waiter({ value: event, done: false });
      } else {
        queue.push(event);
      }
    },
    end() {
      done = true;
      while (waiting.length > 0) {
        const waiter = waiting.shift();
        waiter?.({ done: true });
      }
    },
    async *[Symbol.asyncIterator]() {
      while (true) {
        if (queue.length > 0) {
          yield queue.shift();
        } else if (done) {
          return;
        } else {
          const result = await new Promise<{ value?: StreamEvent; done: boolean }>((resolve) => {
            waiting.push(resolve);
          });
          if (result.done) {
            return;
          }
          yield result.value;
        }
      }
    },
    result() {
      return finalResultPromise;
    },
  };
}

function emptyUsage() {
  return {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
  };
}

function requestedTools(): string[] {
  const raw = process.env.SPIKE_TOOLS ?? "read,write,bash";
  return raw
    .split(",")
    .map((name) => name.trim())
    .filter((name) => name.length > 0);
}

function toolArguments(toolName: string): Record<string, unknown> {
  switch (toolName) {
    case "read":
      return { path: "probe.txt" };
    case "edit":
      return {
        path: "probe.txt",
        edits: [{ oldText: "PROBE_UNCHANGED", newText: "PROBE_EDITED" }],
      };
    case "write":
      return { path: "created.txt", content: "SPIKE_WRITE_OK\n" };
    case "bash":
      return { command: "echo SPIKE_BASH_OK" };
    default:
      throw new Error(`unsupported spike tool: ${toolName}`);
  }
}

function countToolResults(context: { messages?: Array<{ role?: string }> }): number {
  return (context.messages ?? []).filter((message) => message.role === "toolResult").length;
}

type ExtensionAPI = {
  registerProvider: (name: string, config: Record<string, unknown>) => void;
};

export default function (pi: ExtensionAPI) {
  pi.registerProvider("spike", {
    name: "Spike mock provider",
    baseUrl: "http://127.0.0.1",
    apiKey: "spike-no-network",
    api: "spike-tools",
    models: [
      {
        id: "spike-tools",
        name: "Spike Tools",
        reasoning: false,
        input: ["text"],
        contextWindow: 32000,
        maxTokens: 1024,
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      },
    ],
    streamSimple(
      model: { api: string; provider: string; id: string },
      context: { messages?: Array<{ role?: string }> },
    ) {
      const stream = createAssistantStream();
      const output: AssistantMessage = {
        role: "assistant",
        content: [],
        api: model.api,
        provider: model.provider,
        model: model.id,
        usage: emptyUsage(),
        stopReason: "stop",
        timestamp: Date.now(),
      };

      queueMicrotask(() => {
        stream.push({ type: "start", partial: output });
        const tools = requestedTools();
        const next = tools[countToolResults(context)];
        if (next) {
          const args = toolArguments(next);
          const toolCall = {
            type: "toolCall",
            id: `spike-${next}-${countToolResults(context) + 1}`,
            name: next,
            arguments: args,
          };
          output.content.push(toolCall);
          output.stopReason = "toolUse";
          const contentIndex = output.content.length - 1;
          stream.push({ type: "toolcall_start", contentIndex, partial: output });
          stream.push({
            type: "toolcall_end",
            contentIndex,
            toolCall,
            partial: output,
          });
          stream.push({ type: "done", reason: "toolUse", message: output });
        } else {
          output.content.push({ type: "text", text: "SPIKE_DONE" });
          stream.push({
            type: "text_start",
            contentIndex: 0,
            partial: output,
          });
          stream.push({
            type: "text_delta",
            contentIndex: 0,
            delta: "SPIKE_DONE",
            partial: output,
          });
          stream.push({
            type: "text_end",
            contentIndex: 0,
            content: "SPIKE_DONE",
            partial: output,
          });
          stream.push({ type: "done", reason: "stop", message: output });
        }
        stream.end();
      });

      return stream;
    },
  });
}
