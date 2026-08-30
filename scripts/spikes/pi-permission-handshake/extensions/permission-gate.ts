/**
 * Spike-only execution gate. Not loaded by production PiEngineAdapter.
 *
 * Hooks Pi's in-process `tool_call` event (after tool_execution_start, before
 * execute) and asks the RPC client via the generic extension UI confirm dialog.
 * Returning `{ block: true }` prevents the builtin tool from running.
 */

type ToolCallEvent = {
  type: string;
  toolName: string;
  toolCallId: string;
  input: Record<string, unknown>;
};

type ConfirmOptions = {
  timeout?: number;
};

type ExtensionContext = {
  ui: {
    confirm: (
      title: string,
      message: string,
      options?: ConfirmOptions,
    ) => Promise<boolean>;
  };
};

type ExtensionAPI = {
  on: (
    event: "tool_call",
    handler: (
      event: ToolCallEvent,
      ctx: ExtensionContext,
    ) => Promise<{ block: true; reason: string } | undefined>,
  ) => void;
};

export default function (pi: ExtensionAPI) {
  pi.on("tool_call", async (event, ctx) => {
    const confirmed = await ctx.ui.confirm(
      `Allow ${event.toolName}?`,
      JSON.stringify({
        toolCallId: event.toolCallId,
        toolName: event.toolName,
        input: event.input,
      }),
    );
    if (!confirmed) {
      return {
        block: true,
        reason: `spike-denied:${event.toolName}:${event.toolCallId}`,
      };
    }
    return undefined;
  });
}
