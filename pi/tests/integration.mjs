// Runs registered tools through Pi's SDK pipeline, including hooks and MCP.
// Build ilo first; host peer dependencies must be available. No model API calls.
import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  createAgentSession,
  createMcpExtension,
  DefaultResourceLoader,
  SessionManager,
  SettingsManager,
} from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

process.env.ILO_BIN ??= fileURLToPath(new URL("../../target/debug/ilo", import.meta.url));
const cwd = await mkdtemp(join(tmpdir(), "ilo-pi-integration-"));
const agentDir = join(cwd, "agent");
await mkdir(agentDir);
await writeFile(join(agentDir, "mcp.json"), JSON.stringify({
  mcpServers: { ilo: { command: process.env.ILO_BIN, args: ["lsp"], exposure: "direct" } },
}));
const settingsManager = SettingsManager.inMemory({ defaultTools: ["+probe"] });
const resourceLoader = new DefaultResourceLoader({
  cwd, agentDir, settingsManager,
  additionalExtensionPaths: [fileURLToPath(new URL("../extensions/ilo.ts", import.meta.url))],
  extensionFactories: [createMcpExtension(), (pi) => pi.registerTool({
    name: "probe", label: "Probe", description: "Integration assertions",
    parameters: Type.Object({}),
    async execute(_id, _args, signal, _update, ctx) {
      const call = async (name, args) => {
        const outcome = await ctx.executeTool(name, args, { signal });
        assert.equal(outcome.isError, false, JSON.stringify(outcome.result));
        return outcome.result;
      };
      const program = "double x:n>n;*x 2";
      let result = await call("ilo_repl", { command: "send", program, func: "double", args: ["7"] });
      assert.equal(JSON.parse(result.content[0].text).ok, 14);
      console.log("PASS first REPL request skips readiness notification");
      result = await call("ilo_repl", { command: "send", program, func: "double", args: ["11"] });
      assert.equal(JSON.parse(result.content[0].text).ok, 22);
      await call("ilo_repl", { command: "stop" });
      console.log("PASS subsequent REPL request and stop");

      const path = join(cwd, "test.ilo");
      result = await call("write", { path, content: "f x:n>n;xyzz\n" });
      assert(result.content.some(c => c.text?.includes("ILO-T004")));
      assert(result.content.some(c => c.text?.includes("did you mean")));
      console.log("PASS write hook renders readable diagnostics");
      const settle = () => session.extensionRunner.emitBoundary(
        { type: "agent_before_settle", outcome: "success" },
        () => ({ contextEntries: [], contextMessages: [], llmMessages: [], pendingMessages: [], canContinue: true }),
      );
      assert.equal((await settle()).continue, true);
      assert.equal((await settle()).continue, false);
      console.log("PASS dirty-file settle gate nudges once, without looping");
      result = await call("edit", { path, oldText: "xyzz", newText: "x" });
      assert(!result.content.some(c => c.text?.includes("ILO-T004")));
      assert.equal((await settle()).continue, false);
      console.log("PASS clean edit remains silent and clears dirty state");
      // Exercise only the pre-tool hook: no shell command is executed.
      const blocked = await session.extensionRunner.emitToolCall({
        type: "tool_call", toolCallId: "blocked-probe", toolName: "bash",
        input: { command: "ilo example.ilo" },
      });
      assert.equal(blocked.block, true);
      console.log("PASS shell-redirection hook");
      result = await call("ilo_run", { file: path, func: "f", args: ["9"] });
      assert.equal(result.details.exitCode, 0);
      assert(result.content[0].text.includes("9"));
      console.log("PASS file execution");
      result = await call("ilo_help", { topic: "ilo-language" });
      assert(result.content[0].text.length > 100);
      console.log("PASS focused compiler guidance");

      for (let i = 0; i < 100 && !ctx.tools.some(t => t.name === "mcp__ilo__ilo_validate"); i++) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      result = await call("mcp__ilo__ilo_validate", { source: "f x:n>n;xyzz" });
      assert.equal(JSON.parse(result.content[0].text).ok, false);
      console.log("PASS MCP validation through Pi");
      result = await call("mcp__ilo__ilo_fix", { source: "f x:n>n;xyzz" });
      assert.equal(JSON.parse(result.content[0].text).fixed_source, "f x:n>n;x");
      console.log("PASS MCP repair through Pi");
      for (const operation of ["overview", "symbol", "dependencies", "callers"]) {
        result = await call("mcp__ilo__ilo_inspect", { source: program, operation, symbol: "double" });
        assert(!JSON.parse(result.content[0].text).error);
      }
      console.log("PASS all four MCP inspection operations");
      return { content: [{ type: "text", text: "All integration assertions passed" }], details: {} };
    },
  })],
});
await resourceLoader.reload();
const { session } = await createAgentSession({
  cwd, agentDir, resourceLoader, settingsManager, sessionManager: SessionManager.inMemory(),
});
try {
  await session.bindExtensions({});
  // Seed the issuing assistant message required for nested tool calls; no model is invoked.
  session.agent.state.messages.push({
    role: "assistant", content: [{ type: "toolCall", id: "probe-1", name: "probe", arguments: {} }],
    api: "openai-completions", provider: "test", model: "test",
    usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
    stopReason: "toolUse", timestamp: Date.now(),
  });
  const probe = session.agent.state.tools.find(t => t.name === "probe");
  assert(probe, "probe loaded");
  await probe.execute("probe-1", {}, new AbortController().signal);
} finally {
  await session.extensionRunner.emit({ type: "session_shutdown", reason: "exit" });
  session.dispose();
  await rm(cwd, { recursive: true, force: true });
}
