#!/usr/bin/env node
/**
 * Standalone Pi RPC permission-handshake spike (#20).
 *
 * Not part of cargo/vitest. Requires the bundled sidecar JS and a local Node
 * (>=22). Does not need live model credentials. Does not change production
 * --approve / permission_requests behavior.
 *
 * Usage (from repo root):
 *   node scripts/spikes/pi-permission-handshake/run.mjs
 */

import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SPIKE_ROOT = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SPIKE_ROOT, "../../..");
const SIDECAR_JS = join(REPO_ROOT, "src-tauri/binaries/pi-sidecar/dist/piwork-pi.js");
const MOCK_PROVIDER = join(SPIKE_ROOT, "extensions/mock-provider.ts");
const PERMISSION_GATE = join(SPIKE_ROOT, "extensions/permission-gate.ts");
const RECORDED_DIR = join(SPIKE_ROOT, "recorded");

const PRODUCTION_LIKE_FLAGS = [
  "--mode",
  "rpc",
  "--no-skills",
  "--no-prompt-templates",
  "--no-themes",
  "--no-context-files",
  "--offline",
];

const UNKNOWN_PERMISSION_COMMANDS = [
  "permission_request",
  "permission_response",
  "permission",
  "approve",
  "deny",
  "allow",
  "ask",
];

class RpcSession {
  /** @param {import("node:child_process").ChildProcessWithoutNullStreams} child */
  constructor(child, invocation) {
    this.child = child;
    this.invocation = invocation;
    this.buffer = "";
    /** @type {object[]} */
    this.messages = [];
    /** @type {((message: object) => void)[]} */
    this.waiters = [];
    this.stderr = "";
    this.closed = false;

    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      this.buffer += chunk;
      while (true) {
        const newline = this.buffer.indexOf("\n");
        if (newline === -1) {
          return;
        }
        const line = this.buffer.slice(0, newline).replace(/\r$/, "");
        this.buffer = this.buffer.slice(newline + 1);
        if (line.length === 0) {
          continue;
        }
        const message = JSON.parse(line);
        this.messages.push(message);
        const pending = this.waiters.splice(0);
        for (const waiter of pending) {
          waiter(message);
        }
      }
    });
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      this.stderr += chunk;
    });
    child.on("close", () => {
      this.closed = true;
    });
  }

  send(command) {
    this.child.stdin.write(`${JSON.stringify(command)}\n`);
  }

  /**
   * @param {(message: object) => boolean} predicate
   * @param {number} timeoutMs
   * @param {number} [fromIndex]
   */
  async waitFor(predicate, timeoutMs = 15000, fromIndex = 0) {
    const existing = this.messages.slice(fromIndex).find(predicate);
    if (existing) {
      return existing;
    }
    return await new Promise((resolveWait, reject) => {
      const pump = (message) => {
        if (predicate(message)) {
          clearTimeout(timer);
          resolveWait(message);
          return;
        }
        this.waiters.push(pump);
      };
      const timer = setTimeout(() => {
        this.waiters = this.waiters.filter((waiter) => waiter !== pump);
        reject(
          new Error(
            `timeout waiting for RPC message. stderr=${this.stderr.slice(-2000)} last=${JSON.stringify(this.messages.at(-1))}`,
          ),
        );
      }, timeoutMs);
      this.waiters.push(pump);
    });
  }

  async request(command, timeoutMs = 15000) {
    this.send(command);
    return await this.waitFor(
      (message) =>
        message.type === "response" &&
        message.id === command.id &&
        message.command === command.type,
      timeoutMs,
    );
  }

  async close() {
    if (this.closed) {
      return;
    }
    try {
      this.send({ type: "abort" });
    } catch {
      // Process may already be gone.
    }
    this.child.stdin.end();
    await Promise.race([
      new Promise((resolveClose) => this.child.once("close", resolveClose)),
      new Promise((resolveClose) => setTimeout(resolveClose, 2000)),
    ]);
    if (!this.closed) {
      this.child.kill("SIGKILL");
    }
  }
}

async function startSidecar(options) {
  const workspace = options.workspace;
  const agentDir = join(workspace, "agent");
  const sessionDir = join(workspace, "sessions");
  await mkdir(agentDir, { recursive: true });
  await mkdir(sessionDir, { recursive: true });
  await writeFile(join(workspace, "probe.txt"), "PROBE_UNCHANGED\n", "utf8");

  const extensions = options.extensions ?? [];
  const args = [
    SIDECAR_JS,
    ...PRODUCTION_LIKE_FLAGS,
    "--provider",
    options.provider ?? "spike",
    "--model",
    options.model ?? "spike-tools",
    "--session-dir",
    sessionDir,
    "--session-id",
    options.sessionId,
    "--tools",
    options.tools ?? "read,grep,find,ls,edit,write,bash",
    "--no-extensions",
    options.approve === false ? "--no-approve" : "--approve",
  ];
  for (const extension of extensions) {
    args.push("--extension", extension);
  }

  const child = spawn(process.execPath, args, {
    cwd: workspace,
    env: {
      ...process.env,
      HOME: workspace,
      PI_CODING_AGENT_DIR: agentDir,
      PI_CODING_AGENT_SESSION_DIR: sessionDir,
      SPIKE_TOOLS: options.spikeTools ?? "read,write,bash",
      PI_OFFLINE: "1",
    },
    stdio: ["pipe", "pipe", "pipe"],
  });

  const invocation = {
    program: "node",
    args: args.map((value) =>
      value.startsWith(REPO_ROOT) ? value.slice(REPO_ROOT.length + 1) : value,
    ),
    cwd: "<isolated-temp-workspace>",
    env: {
      PI_CODING_AGENT_DIR: "<workspace>/agent",
      PI_CODING_AGENT_SESSION_DIR: "<workspace>/sessions",
      SPIKE_TOOLS: options.spikeTools ?? "read,write,bash",
      PI_OFFLINE: "1",
    },
  };
  const session = new RpcSession(child, invocation);
  await session.waitUntilReady();
  return session;
}

RpcSession.prototype.waitUntilReady = async function waitUntilReady() {
  const deadline = Date.now() + 20000;
  let attempt = 0;
  while (Date.now() < deadline) {
    if (this.closed) {
      throw new Error(`sidecar exited during startup. stderr=${this.stderr.slice(-2000)}`);
    }
    attempt += 1;
    const id = `ready-${attempt}`;
    this.send({ id, type: "get_state" });
    try {
      const response = await this.waitFor(
        (message) => message.type === "response" && message.id === id,
        2000,
      );
      if (response.success) {
        return response;
      }
    } catch {
      // Process may still be loading the 13MB bundle.
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 200));
  }
  throw new Error(`sidecar did not accept get_state. stderr=${this.stderr.slice(-2000)}`);
};

async function inventoryCommands(session) {
  const unknown = [];
  for (const [index, type] of UNKNOWN_PERMISSION_COMMANDS.entries()) {
    const response = await session.request({
      id: `inventory-${index + 1}`,
      type,
    });
    unknown.push(response);
  }
  const getState = await session.request({ id: "inventory-state", type: "get_state" });
  const getCommands = await session.request({ id: "inventory-commands", type: "get_commands" });
  return { unknown, getState, getCommands };
}

async function promptUntilSettled(session, requestId, autoRespond) {
  let cursor = session.messages.length;
  session.send({
    id: requestId,
    type: "prompt",
    message: "Exercise builtin tools.",
  });
  const accepted = await session.waitFor(
    (message) => message.type === "response" && message.id === requestId,
    15000,
    cursor,
  );
  cursor = session.messages.indexOf(accepted) + 1;
  while (true) {
    const message = await session.waitFor(() => true, 20000, cursor);
    cursor = session.messages.indexOf(message) + 1;
    if (message.type === "extension_ui_request" && message.method === "confirm") {
      const decision = autoRespond(message);
      session.send({
        type: "extension_ui_response",
        id: message.id,
        confirmed: decision,
      });
    }
    if (message.type === "agent_settled") {
      break;
    }
    if (message.type === "agent_end") {
      const settled = session.messages.slice(cursor).some((item) => item.type === "agent_settled");
      if (settled) {
        continue;
      }
      await new Promise((resolveWait) => setTimeout(resolveWait, 300));
      if (session.messages.slice(cursor).some((item) => item.type === "agent_settled")) {
        continue;
      }
      break;
    }
  }
  return accepted;
}

function summarizeMessages(messages) {
  return messages.map((message) => {
    if (message.type === "extension_ui_request") {
      return {
        type: message.type,
        id: message.id,
        method: message.method,
        title: message.title,
        message: message.message,
      };
    }
    if (message.type === "tool_execution_start") {
      return {
        type: message.type,
        toolCallId: message.toolCallId,
        toolName: message.toolName,
        args: message.args,
      };
    }
    if (message.type === "tool_execution_end") {
      return {
        type: message.type,
        toolCallId: message.toolCallId,
        toolName: message.toolName,
        isError: message.isError ?? false,
        result: message.result,
      };
    }
    if (message.type === "response") {
      return {
        type: message.type,
        id: message.id,
        command: message.command,
        success: message.success,
        error: message.error,
      };
    }
    return { type: message.type };
  });
}

async function runScenario(name, options) {
  const workspace = await mkdtemp(join(tmpdir(), `pi-handshake-${name}-`));
  const session = await startSidecar({
    workspace,
    sessionId: `spike-${name}`,
    ...options,
  });
  try {
    if (options.inventory) {
      const inventory = await inventoryCommands(session);
      return {
        name,
        invocation: session.invocation,
        inventory,
        messages: summarizeMessages(session.messages),
        raw: session.messages,
        stderr: session.stderr,
        workspace,
      };
    }

    const accepted = await promptUntilSettled(
      session,
      `prompt-${name}`,
      options.autoRespond ?? (() => true),
    );
    const probe = await readFile(join(workspace, "probe.txt"), "utf8").catch(() => "");
    const created = await readFile(join(workspace, "created.txt"), "utf8").catch(() => null);
    return {
      name,
      invocation: session.invocation,
      accepted,
      messages: summarizeMessages(session.messages),
      raw: session.messages,
      stderr: session.stderr,
      workspace,
      files: { probe, created },
    };
  } finally {
    await session.close();
    if (!options.keepWorkspace) {
      await rm(workspace, { recursive: true, force: true });
    }
  }
}

async function helpText() {
  const child = spawn(process.execPath, [SIDECAR_JS, "--help"], {
    cwd: REPO_ROOT,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "";
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  await new Promise((resolveClose) => child.on("close", resolveClose));
  const lines = stdout.split("\n").filter((line) => {
    const lower = line.toLowerCase();
    return (
      lower.includes("approve") ||
      lower.includes("permission") ||
      lower.includes("--mode") ||
      lower.includes("--tools") ||
      lower.includes("--extension")
    );
  });
  return { lines, hasPermissionFlag: /--permission/i.test(stdout) };
}

async function main() {
  const sidecarSource = await readFile(SIDECAR_JS, "utf8");
  const sidecarHasApproveAsProjectTrust = sidecarSource.includes(
    "--approve, -a                  Trust project-local files for this run",
  );
  const help = await helpText();

  const inventory = await runScenario("inventory", {
    inventory: true,
    extensions: [MOCK_PROVIDER],
    spikeTools: "read",
  });

  const noGate = await runScenario("no-gate-builtin-tools", {
    extensions: [MOCK_PROVIDER],
    spikeTools: "read,edit,write,bash",
  });

  const allow = await runScenario("gate-allow-write", {
    extensions: [MOCK_PROVIDER, PERMISSION_GATE],
    spikeTools: "write",
    autoRespond: () => true,
  });

  const deny = await runScenario("gate-deny-write", {
    extensions: [MOCK_PROVIDER, PERMISSION_GATE],
    spikeTools: "write",
    autoRespond: () => false,
  });

  const nativePermissionEvents = ["permission_request", "permission_requested", "permission"];
  const noGateNativePermission = noGate.raw.filter((message) =>
    nativePermissionEvents.includes(message.type),
  );
  const noGateUiRequests = noGate.raw.filter((message) => message.type === "extension_ui_request");
  const allowUiRequests = allow.raw.filter((message) => message.type === "extension_ui_request");
  const denyBlocked = deny.raw.some(
    (message) =>
      message.type === "tool_execution_end" &&
      message.isError === true &&
      JSON.stringify(message.result ?? {}).includes("spike-denied"),
  );

  const conclusion = {
    sidecar: {
      package: "@earendil-works/pi-coding-agent",
      version: "0.80.2",
      entry: "src-tauri/binaries/pi-sidecar/dist/piwork-pi.js",
      node: "node",
      nodeVersion: process.version,
      approveFlagMeansProjectTrust: sidecarHasApproveAsProjectTrust,
      helpLines: help.lines,
      hasPermissionCliFlag: help.hasPermissionFlag,
    },
    rustAdapterNote:
      "PiEngineAdapter::production is fail-closed on Linux; this spike invokes the JS sidecar directly.",
    nativeHandshake: {
      emitsPermissionRequestBeforeBuiltinTools: false,
      externalAllowDenyCommandExists: false,
      evidence: {
        unknownPermissionCommands: inventory.inventory.unknown,
        noGateNativePermissionEvents: noGateNativePermission,
        noGateExtensionUiRequests: noGateUiRequests.length,
        noGateCreatedFile: noGate.files.created,
      },
    },
    extensionMediatedHandshake: {
      available: allowUiRequests.length > 0 && denyBlocked && deny.files.created === null,
      requestType: "extension_ui_request",
      responseType: "extension_ui_response",
      allowObserved: allowUiRequests[0] ?? null,
      denyBlocked,
      denyCreatedFile: deny.files.created,
    },
    invocations: {
      inventory: inventory.invocation,
      noGate: noGate.invocation,
      allow: allow.invocation,
      deny: deny.invocation,
    },
    scenarios: {
      inventory: {
        messages: inventory.messages,
      },
      noGate: {
        accepted: noGate.accepted,
        files: noGate.files,
        messages: noGate.messages,
      },
      allow: {
        accepted: allow.accepted,
        files: allow.files,
        messages: allow.messages,
      },
      deny: {
        accepted: deny.accepted,
        files: deny.files,
        messages: deny.messages,
      },
    },
  };

  await mkdir(RECORDED_DIR, { recursive: true });
  const outputPath = join(RECORDED_DIR, "latest.json");
  await writeFile(outputPath, `${JSON.stringify(conclusion, null, 2)}\n`, "utf8");
  process.stdout.write(`${JSON.stringify(conclusion, null, 2)}\n`);
  process.stdout.write(`\nWrote ${outputPath}\n`);

  if (!sidecarHasApproveAsProjectTrust) {
    throw new Error("bundled sidecar help/source no longer describes --approve as project trust");
  }
  if (help.hasPermissionFlag) {
    throw new Error("bundled sidecar now exposes a --permission flag; update the decision note");
  }
  if (inventory.inventory.unknown.some((response) => response.success === true)) {
    throw new Error("a permission-like RPC command was accepted; update the decision note");
  }
  if (noGateNativePermission.length > 0 || noGateUiRequests.length > 0) {
    throw new Error("no-gate run emitted a permission or UI request");
  }
  if (noGate.files.probe !== "PROBE_EDITED\n") {
    throw new Error(`no-gate edit did not execute, probe=${JSON.stringify(noGate.files.probe)}`);
  }
  if (noGate.files.created !== "SPIKE_WRITE_OK\n") {
    throw new Error(`no-gate write did not execute, got ${JSON.stringify(noGate.files.created)}`);
  }
  if (!allowUiRequests.length || allow.files.created !== "SPIKE_WRITE_OK\n") {
    throw new Error("gate-allow did not confirm then execute write");
  }
  if (!denyBlocked || deny.files.created !== null) {
    throw new Error("gate-deny did not block write before execution");
  }
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exit(1);
});
