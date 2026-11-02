import type {
	ExtensionAPI,
	ToolResultEvent,
	ToolResultEventResult,
} from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync } from "node:fs";
import { delimiter, dirname, isAbsolute, join, resolve as resolvePath } from "node:path";
import { fileURLToPath } from "node:url";

type Details = Record<string, unknown>;
type ToolResult = {
	content: Array<{ type: "text"; text: string }>;
	details: Details;
};

const REPL_IDLE_TIMEOUT_MS = 5 * 60 * 1000;
const REPL_REQUEST_TIMEOUT_MS = 30 * 1000;

interface PendingRequest {
	resolve: (value: string) => void;
	reject: (err: Error) => void;
}

interface ReplState {
	process: ChildProcessWithoutNullStreams;
	cwd: string;
	buffer: string;
	idleTimer: NodeJS.Timeout | null;
	killTimer: NodeJS.Timeout | null;
	pending: PendingRequest[];
}

let repl: ReplState | null = null;

function resolveIloBinary(): string {
	const fromEnv = process.env.ILO_BIN;
	if (fromEnv && existsSync(fromEnv)) return fromEnv;

	const extDir = dirname(fileURLToPath(import.meta.url));
	const bundled = resolvePath(extDir, "..", "node_modules", ".bin", "ilo");
	if (existsSync(bundled)) return bundled;

	// A checkout build comes before PATH: it is the binary the extension is
	// usually developed against, and an older `ilo` earlier on PATH would
	// silently lack newer subcommands (e.g. `ilo fix`). When the extension
	// is installed from npm there is no checkout, so PATH is the fallback.
	const executable = process.platform === "win32" ? "ilo.exe" : "ilo";
	const checkoutBuild = resolvePath(extDir, "../../target/debug", executable);
	if (existsSync(checkoutBuild)) return checkoutBuild;

	for (const directory of (process.env.PATH ?? "").split(delimiter)) {
		if (!directory) continue;
		const candidate = join(directory, executable);
		if (existsSync(candidate)) return candidate;
	}
	return "ilo";
}

function runIlo(args: string[], stdin?: string, signal?: AbortSignal, cwd?: string): Promise<{
	stdout: string;
	stderr: string;
	exitCode: number | null;
}> {
	return new Promise((resolve, reject) => {
		const child = spawn(resolveIloBinary(), args, {
			stdio: ["pipe", "pipe", "pipe"],
			...(cwd ? { cwd } : {}),
		});

		let stdout = "";
		let stderr = "";
		let forceKillTimer: NodeJS.Timeout | undefined;
		const cleanup = () => {
			if (forceKillTimer) clearTimeout(forceKillTimer);
			signal?.removeEventListener("abort", onAbort);
		};
		const onAbort = () => {
			child.kill("SIGTERM");
			forceKillTimer = setTimeout(() => child.kill("SIGKILL"), 1_000);
			forceKillTimer.unref();
		};
		if (signal?.aborted) onAbort();
		else signal?.addEventListener("abort", onAbort, { once: true });

		child.stdout.on("data", (chunk) => {
			stdout += chunk.toString();
		});
		child.stderr.on("data", (chunk) => {
			stderr += chunk.toString();
		});

		child.on("error", (error) => {
			cleanup();
			reject(error);
		});
		child.on("close", (exitCode) => {
			cleanup();
			resolve({ stdout, stderr, exitCode });
		});

		// Swallow EPIPE if the child exits before we finish writing stdin.
		// Without this, the 'error' event becomes uncaught.
		child.stdin.on("error", () => {});

		try {
			if (stdin !== undefined) child.stdin.write(stdin);
			child.stdin.end();
		} catch {
			// Child already exited; close handler will resolve with its exit code.
		}
	});
}

function resetReplIdleTimer() {
	const state = repl;
	if (!state) return;
	if (state.idleTimer) clearTimeout(state.idleTimer);
	state.idleTimer = setTimeout(() => {
		// A timer from an old process must never stop a newer session.
		if (repl === state) stopRepl();
	}, REPL_IDLE_TIMEOUT_MS);
}

function stopRepl(reason = new Error("REPL session stopped before response arrived")) {
	const state = repl;
	if (!state) return;
	repl = null;
	if (state.idleTimer) clearTimeout(state.idleTimer);
	for (const pending of state.pending.splice(0)) pending.reject(reason);
	try {
		state.process.kill("SIGTERM");
		state.killTimer = setTimeout(() => state.process.kill("SIGKILL"), 1_000);
		state.killTimer.unref();
	} catch {
		// ignore
	}
}

function startRepl(cwd: string): ReplState {
	if (repl?.cwd === cwd) return repl;
	if (repl) stopRepl(new Error("ilo serv working directory changed; session reset"));

	const proc = spawn(resolveIloBinary(), ["serv"], {
		stdio: ["pipe", "pipe", "pipe"],
		cwd,
	});

	const state: ReplState = {
		process: proc,
		cwd,
		buffer: "",
		idleTimer: null,
		killTimer: null,
		pending: [],
	};
	repl = state;

	// `ilo serv` speaks newline-delimited JSON: one request line in, one response
	// line out. If a line arrives with no pending request it means the protocol
	// has drifted (or the child emitted something unexpected); surface it on
	// stderr instead of silently consuming it.
	proc.stdout.on("data", (chunk: Buffer) => {
		state.buffer += chunk.toString();
		let newlineIndex = state.buffer.indexOf("\n");
		while (newlineIndex !== -1) {
			const line = state.buffer.slice(0, newlineIndex);
			state.buffer = state.buffer.slice(newlineIndex + 1);
			// Startup notifications are not responses to queued requests.
			try {
				const message = JSON.parse(line);
				if (message?.ready === true && !("ok" in message) && !("error" in message)) {
					newlineIndex = state.buffer.indexOf("\n");
					continue;
				}
			} catch { /* Preserve non-JSON responses for error reporting. */ }
			const pending = state.pending.shift();
			if (pending) {
				pending.resolve(line);
			} else if (line.length > 0) {
				process.stderr.write(`pi-ilo-lang: orphan response from ilo serv: ${line}\n`);
			}
			newlineIndex = state.buffer.indexOf("\n");
		}
	});

	// Swallow EPIPE on stdin if the child exits. The actual rejection of any
	// in-flight pendings is handled by the process 'close' / 'error' handlers
	// below, which see all of them, not just the most recent.
	proc.stdin.on("error", () => {});

	proc.on("close", () => {
		if (state.idleTimer) clearTimeout(state.idleTimer);
		if (state.killTimer) clearTimeout(state.killTimer);
		for (const pending of state.pending.splice(0)) {
			pending.reject(new Error("ilo serv exited"));
		}
		state.pending = [];
		if (repl === state) repl = null;
	});

	proc.on("error", (err) => {
		if (state.idleTimer) clearTimeout(state.idleTimer);
		if (state.killTimer) clearTimeout(state.killTimer);
		for (const pending of state.pending.splice(0)) {
			pending.reject(err);
		}
		state.pending = [];
		if (repl === state) repl = null;
	});

	resetReplIdleTimer();
	return state;
}

function sendToRepl(payload: object, signal?: AbortSignal, cwd = process.cwd()): Promise<string> {
	if (signal?.aborted) return Promise.reject(new Error("ilo serv request aborted"));
	const state = repl?.cwd === cwd ? repl : startRepl(cwd);
	resetReplIdleTimer();

	return new Promise<string>((resolve, reject) => {
		if (state.process.exitCode !== null || !state.process.stdin.writable) {
			reject(new Error("ilo serv session is not writable"));
			return;
		}

		const timer = setTimeout(() => {
			// A response may still arrive after timeout. Retiring the session
			// prevents that stale line being paired with a later request.
			if (repl === state) stopRepl(new Error("ilo serv request timed out; session reset"));
			else entry.reject(new Error("ilo serv request timed out"));
		}, REPL_REQUEST_TIMEOUT_MS);

		const entry: PendingRequest = {
			resolve: (value) => {
				clearTimeout(timer);
				signal?.removeEventListener("abort", onAbort);
				resolve(value);
			},
			reject: (err) => {
				clearTimeout(timer);
				signal?.removeEventListener("abort", onAbort);
				reject(err);
			},
		};

		const onAbort = () => {
			// Do not remove only this queue slot: the corresponding response can
			// still arrive and would then resolve the next caller's request.
			if (repl === state) stopRepl(new Error("ilo serv request aborted; session reset"));
			else entry.reject(new Error("ilo serv request aborted"));
		};
		if (signal) signal.addEventListener("abort", onAbort, { once: true });

		state.pending.push(entry);

		try {
			state.process.stdin.write(JSON.stringify(payload) + "\n");
		} catch (err) {
			const index = state.pending.indexOf(entry);
			if (index !== -1) state.pending.splice(index, 1);
			entry.reject(err instanceof Error ? err : new Error(String(err)));
		}
	});
}

function describeIloError(err: unknown): string {
	const message = err instanceof Error ? err.message : String(err);
	if ((err as NodeJS.ErrnoException)?.code === "ENOENT") {
		return `ilo binary not found. Install ilo (https://github.com/ilo-lang/ilo) or set ILO_BIN to an explicit path. (${message})`;
	}
	return message;
}

// ---------------------------------------------------------------------------
// Hooks: invariants enforced by mechanism, not by skill prose.
//
// Design rule: skills carry *knowledge* (what `?{...}` arms mean); hooks
// carry *invariants* (validate after editing, never shell out to `ilo`).
// Every "always/never" sentence that would otherwise live in SKILL.md is a
// candidate for extraction into a handler below. Hooks are fail-open: any
// internal error returns undefined and the turn proceeds untouched.
//
// Cross-harness equivalents (Pi only is implemented; logic is portable):
// - validate-on-edit `tool_result` handler → Claude Code `PostToolUse`
//   with matcher `Edit|Write` (or `Bash`), appending to `additionalContext`;
//   Codex `[[hooks]]` on the equivalent post-tool event.
// - run-failure enrichment → Claude Code `PostToolUse` matcher on the
//   run tool, same `additionalContext` channel.
// - `bash` redirect `tool_call` block → Claude Code `PreToolUse` matcher
//   `Bash` returning `"decision": "block"` with a reason.
// - settle gate (`agent_before_settle` + `continue: true`) → Claude Code
//   `Stop` hook returning a block-with-reason, forcing continuation.
// - `registerMcpServer("ilo", ...)` bridge → `claude mcp add` /
//   Codex `mcp_servers` config (native in both; no code needed).
//
// Disable all hooks (tools keep working) with ILO_NO_HOOKS=1.
// ---------------------------------------------------------------------------

const ILO_HOOKS_DISABLED =
	process.env.ILO_NO_HOOKS === "1" || process.env.ILO_NO_HOOKS === "true";
const ILO_CHECK_TIMEOUT_MS = 15_000;
const ILO_DIAG_SNIPPET_MAX = 2000;

/** .ilo files edited but not yet cleanly validated. Cleared on session_start. */
const dirtyIloFiles = new Set<string>();
/** Settle-gate loop guard: at most one forced continuation per user request. */
let settleNudgedTurnIndex: number | null = null;
let currentTurnIndex = 0;

function iloPathFromInput(input: Record<string, unknown>, cwd: string): string | null {
	const raw = input.path ?? input.file;
	if (typeof raw !== "string" || !raw.endsWith(".ilo")) return null;
	return isAbsolute(raw) ? raw : resolvePath(cwd, raw);
}

function iloPathsFromCommand(command: unknown, cwd: string): string[] {
	if (typeof command !== "string" || !command.includes(".ilo")) return [];
	const found = new Set<string>();
	for (const match of command.matchAll(/["']?([\w./~$-][\w./~$-]*\.ilo)["']?/g)) {
		try {
			const path = isAbsolute(match[1]) ? match[1] : resolvePath(cwd, match[1]);
			if (existsSync(path)) found.add(path);
		} catch {
			// ignore; existence check is best-effort
		}
	}
	return [...found];
}

/** Parse the JSON diagnostic stream emitted by `ilo check --json`.
 *  Returns the diagnostics that carry a `fix_plan` first-class so callers
 *  can decide whether a mechanical repair is available. */
function parseDiagnosticJson(output: string): Array<Record<string, unknown>> {
	const diagnostics: Array<Record<string, unknown>> = [];
	for (const line of output.split("\n")) {
		const trimmed = line.trim();
		if (!trimmed.startsWith("{")) continue;
		try {
			const parsed = JSON.parse(trimmed);
			if (parsed && typeof parsed === "object" && typeof parsed.message === "string") {
				diagnostics.push(parsed as Record<string, unknown>);
			}
		} catch { /* not a diagnostic line */ }
	}
	return diagnostics;
}

function readableDiagnostics(output: string): string {
	return output.split("\n").map((line) => {
		try {
			const d = JSON.parse(line);
			if (typeof d.message !== "string") return line;
			const label = d.labels?.find((l: { primary?: boolean }) => l.primary);
			const location = label?.line ? `${label.line}:${label.col ?? 1} ` : "";
			return `${location}${d.code ?? d.severity ?? "diagnostic"}: ${d.message}${d.suggestion ? `\n  → ${d.suggestion}` : ""}`;
		} catch { return line; }
	}).join("\n");
}

/** Run `ilo check` on an input. Returns compact diagnostics, or null when clean. Never throws. */
async function checkIlo(args: string[], dirtyPath?: string): Promise<string | null> {
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), ILO_CHECK_TIMEOUT_MS);
	try {
		const result = await runIlo(["check", ...args], undefined, controller.signal);
		const output = `${result.stdout}\n${result.stderr}`.trim();
		if (result.exitCode === 0 && output === "" && dirtyPath) dirtyIloFiles.delete(dirtyPath);
		if (result.exitCode === 0 && output === "") return null;
		const readable = readableDiagnostics(output);
		const head =
			readable.length > ILO_DIAG_SNIPPET_MAX
				? readable.slice(0, ILO_DIAG_SNIPPET_MAX) + "\n\u2026(truncated)"
				: readable;
		// A failing file check leaves its dirty marker set for the settle gate.
		return head === "" ? `(exit ${result.exitCode}, no output)` : head;
	} catch {
		return null;
	} finally {
		clearTimeout(timer);
	}
}

function checkIloFile(path: string): Promise<string | null> {
	return checkIlo([path], path);
}

function checkIloSource(source: string): Promise<string | null> {
	return checkIlo([source]);
}

function markDirty(path: string): void {
	dirtyIloFiles.add(path);
}

function appendTextResult(
	event: ToolResultEvent,
	suffix: string,
): ToolResultEventResult {
	return {
		content: [...event.content, { type: "text", text: suffix }],
		...(event.structuredContent !== undefined ? { structuredContent: event.structuredContent } : {}),
	};
}

function hasFixPlan(structured: unknown): { has: boolean; code?: string } {
	try {
		const v = structured as { diagnostics?: Array<{ code?: string; fix_plan?: unknown }> };
		const d = v?.diagnostics?.find((x) => x?.fix_plan != null);
		return d ? { has: true, code: d.code } : { has: false };
	} catch { return { has: false }; }
}

function registerIloHooks(pi: ExtensionAPI): void {
	if (ILO_HOOKS_DISABLED) return;

	pi.on("session_start", () => {
		dirtyIloFiles.clear();
		settleNudgedTurnIndex = null;
	});

	pi.on("before_agent_start", () => {
		settleNudgedTurnIndex = null;
	});

	pi.on("turn_start", (event) => {
		currentTurnIndex = event.turnIndex;
	});

	// Validate-on-edit: errors surface in the same turn, no model step needed.
	pi.on("tool_result", async (event, ctx) => {
		try {
			if (event.toolName === "edit" || event.toolName === "write") {
				const path = iloPathFromInput(event.input, ctx.cwd);
				if (!path) return;
				markDirty(path);
				const diagnostics = await checkIloFile(path);
				if (diagnostics === null) return;
				return appendTextResult(event, `[ilo] validation of ${path}:\n${diagnostics}`);
			}
			if (event.toolName === "bash") {
				const paths = iloPathsFromCommand(event.input.command, ctx.cwd);
				if (paths.length === 0) return;
				const sections: string[] = [];
				for (const path of paths) {
					markDirty(path);
					const diagnostics = await checkIloFile(path);
					if (diagnostics !== null) sections.push(`[ilo] validation of ${path}:\n${diagnostics}`);
				}
				if (sections.length === 0) return;
				return appendTextResult(event, sections.join("\n\n"));
			}
			// Every MCP validation result can carry a fix_plan. Surface it
			// immediately so the model knows the cheapest path.
			if ((event.toolName === "ilo_validate" || /^mcp__.+__ilo_validate$/.test(event.toolName))) {
				// Hook .ilo edits: a validation of a dirty file should satisfy
				// the settle gate, so re-check the on-disk file rather than
				// trusting a possibly stale source string in the MCP args.
				const path = iloPathFromInput(event.input, ctx.cwd);
				if (path && dirtyIloFiles.has(path)) {
					const diagnostics = await checkIloFile(path);
					if (diagnostics !== null) return appendTextResult(event, `[ilo] current file diagnostics for ${path}:\n${diagnostics}`);
					return;
				}
				const fp = hasFixPlan(event.structuredContent);
				if (fp.has) {
					const hint = fp.code === "ILO-P011" ? " (reserved name → use hyphen or digit suffix)" : "";
					return appendTextResult(event, `[ilo] → ⚡ fix available: call ilo_apply_fix with same source (index 0) or ilo_fix for multi-fix loop${hint}`);
				}
				return;
			}
			if ((event.toolName === "ilo_fix" || /^mcp__.+__ilo_fix$/.test(event.toolName)) && event.structuredContent) {
				const v = event.structuredContent as Record<string, unknown>;
				const remaining = typeof v.remaining_fixable === "number" ? v.remaining_fixable : null;
				if (remaining !== null && remaining > 0) return appendTextResult(event, `[ilo] → ${remaining} fixable diagnostic(s) remain — re-validate before finishing.`);
			}
			if (event.toolName === "ilo_run") {
				const details = event.details;
				const exitCode =
					details !== null && typeof details === "object" && "exitCode" in details
						? details.exitCode
						: undefined;
				const failed = event.isError || (typeof exitCode === "number" && exitCode !== 0);
				const file =
					typeof event.input.file === "string" && event.input.file.endsWith(".ilo")
						? iloPathFromInput(event.input, ctx.cwd)
						: null;
				if (file) {
					// A successful file run also validates and clears any dirty marker.
					const diagnostics = await checkIloFile(file);
					if (diagnostics !== null && failed) {
						return appendTextResult(event, `[ilo] verifier diagnostics for ${file}:\n${diagnostics}`);
					}
					if (diagnostics !== null) {
						return appendTextResult(event, `[ilo] validation of ${file}:\n${diagnostics}`);
					}
					return;
				}
				if (failed && typeof event.input.code === "string") {
					const diagnostics = await checkIloSource(event.input.code);
					if (diagnostics !== null) {
						return appendTextResult(event, `[ilo] verifier diagnostics for inline source:\n${diagnostics}`);
					}
				}
			}
		} catch {
			// fail-open: never break a turn from a hook
		}
	});

	// Redirect: `ilo` over bash is always worse than the native tools.
	// Blocks the call with a reason so the model retries via ilo_run.
	pi.on("tool_call", (event) => {
		try {
			if (ILO_HOOKS_DISABLED || event.toolName !== "bash") return;
			const command = event.input.command;
			if (typeof command !== "string") return;
			// Match command positions (start of input / after shell separators),
			// not arbitrary words inside `echo`, comments, or quoted arguments.
			const iloCommand =
				/(?:^|[\n;&|])\s*(?:(?:env|command|exec|sudo)\s+)*(?:[A-Za-z_][A-Za-z0-9_]*=[^\s]+\s+)*(?:(?:env|command|exec|sudo)\s+)*(?:[A-Za-z_][A-Za-z0-9_]*=[^\s]+\s+)*(?:"(?:[^"]*\/)?ilo(?:\.exe)?"|'(?:[^']*\/)?ilo(?:\.exe)?'|(?:[^\s"';&|]*\/)?ilo(?:\.exe)?)(?=\s|$|[;&|])/m;
			if (iloCommand.test(command)) {
				return {
					block: true,
					reason:
						"Do not shell out to the `ilo` binary; use the ilo_run tool (or ilo_repl for sessions) instead: structured results, no permission prompt.",
				};
			}
		} catch {
			// fail-open
		}
	});

	// Settle gate: never finish a run with unvalidated .ilo edits.
	pi.on("agent_before_settle", async () => {
		try {
			if (dirtyIloFiles.size === 0) return;
			if (settleNudgedTurnIndex !== null) return;
			settleNudgedTurnIndex = currentTurnIndex;
			const files = [...dirtyIloFiles].join(", ");
			return {
				continue: true,
				entries: [
					{
						type: "custom_message",
						customType: "ilo-unvalidated-edits",
						content: `You edited ${files} without a clean validation. Run each file with ilo_run or validate its exact path with ilo, then fix any remaining diagnostics before finishing.`,
						display: false,
					},
				],
			};
		} catch {
			// fail-open
		}
	});

	// NOTE: an earlier version bridged the compiler's MCP server into the
	// session with `pi.registerMcpServer(...)`. That API does not exist in
	// the pi SDK this extension targets, so the bridge was silently a no-op
	// and the `ilo_validate` / `ilo_fix` tools never reached the agent.
	//
	// The language-intelligence tools are therefore registered directly
	// below (see `ilo_check` and `ilo_fix`), which works on every pi
	// version. Users who want the full MCP surface (hover, completion,
	// go-to-definition, inspect) can still register the compiler manually:
	//
	//   pi mcp add ilo -- /absolute/path/to/ilo lsp
	//
	// or by adding to the agent's mcp.json:
	//
	//   {"mcpServers":{"ilo":{"command":"/abs/path/to/ilo","args":["lsp"],"exposure":"direct"}}}
}

export default function (pi: ExtensionAPI) {
	registerIloHooks(pi);
	pi.on("session_shutdown", () => stopRepl());
	pi.registerTool({
		name: "ilo_help",
		label: "ilo guidance",
		description: "Read focused, version-matched guidance from the installed ilo compiler before writing code. Start with ilo-language and ilo-edit-loop; request one topic at a time.",
		promptGuidelines: ["Before writing unfamiliar ilo syntax, use ilo_help for ilo-language and ilo-edit-loop; do not guess from another language."],
		parameters: Type.Object({ topic: Type.Union([
			Type.Literal("ilo-language"), Type.Literal("ilo-edit-loop"),
			Type.Literal("ilo-builtins"), Type.Literal("ilo-errors"),
		]) }),
		async execute(_id, params, signal, _update, ctx): Promise<ToolResult> {
			const result = await runIlo(["skill", "get", params.topic], undefined, signal, ctx.cwd);
			if (result.exitCode !== 0) throw new Error(result.stderr || "Compiler guidance unavailable; check ILO_BIN and compiler version.");
			return { content: [{ type: "text", text: result.stdout }], details: { topic: params.topic } };
		},
	});
	pi.registerTool({
		name: "ilo_check",
		label: "Check ilo source",
		description:
			"Verify an ilo program WITHOUT running it. Pass `code` for inline source or `file` for a .ilo path. Returns every diagnostic with its code, location, and (where the compiler can prove it) a mechanical `fix_plan`. A diagnostic carrying a fix_plan can be repaired with ilo_fix. Prefer this over running a program to find out whether it compiles.",
		promptGuidelines: ["After writing or editing ilo code, call ilo_check to see diagnostics before running it."],
		parameters: Type.Object({
			code: Type.Optional(Type.String({
				description: "Inline ilo source. Mutually exclusive with `file`.",
			})),
			file: Type.Optional(Type.String({
				description: "Path to a .ilo file. Mutually exclusive with `code`.",
			})),
		}),

		async execute(_toolCallId, params, signal, _onUpdate, ctx): Promise<ToolResult> {
			const code = params.code as string | undefined;
			const file = params.file as string | undefined;
			if (!code && !file) {
				return {
					content: [{ type: "text", text: "Error: provide either `code` or `file`." }],
					details: { error: "Missing `code` or `file`" },
				};
			}
			if (code && file) {
				return {
					content: [{ type: "text", text: "Error: provide `code` OR `file`, not both." }],
					details: { error: "`code` and `file` are mutually exclusive" },
				};
			}

			try {
				// `ilo check --json` emits one JSON diagnostic per line on
				// stderr; a clean program exits 0 with no output.
				const result = await runIlo(["check", "--json", code ?? (file as string)], undefined, signal, ctx.cwd);
				const diagnostics = parseDiagnosticJson(result.stderr);
				const ok = result.exitCode === 0 && diagnostics.length === 0;
				return {
					content: [{
						type: "text",
						text: ok
							? "No diagnostics. The program verifies."
							: readableDiagnostics(result.stderr),
					}],
					details: { ok, diagnostics, exitCode: result.exitCode },
				};
			} catch (err) {
				const message = describeIloError(err);
				return {
					content: [{ type: "text", text: `Error: ${message}` }],
					details: { error: message },
				};
			}
		},
	});

	pi.registerTool({
		name: "ilo_fix",
		label: "Fix ilo source",
		description:
			"Apply the compiler's mechanical, type-proven repairs to an ilo program and return the fixed source. Pass `code` for inline source or `file` for a .ilo path. Only edits the verifier can prove safe are applied; anything else is left alone and reported as a remaining diagnostic. Nothing is written to disk — you decide whether to apply the returned source.",
		promptGuidelines: ["When ilo_check reports a fix_plan, call ilo_fix to get the corrected source instead of hand-editing."],
		parameters: Type.Object({
			code: Type.Optional(Type.String({
				description: "Inline ilo source. Mutually exclusive with `file`.",
			})),
			file: Type.Optional(Type.String({
				description: "Path to a .ilo file. Mutually exclusive with `code`.",
			})),
			max_iters: Type.Optional(Type.Number({
				description: "Maximum repair iterations to apply (default 10).",
			})),
			safe_only: Type.Optional(Type.Boolean({
				description: "Only apply repairs the verifier's types prove safe.",
			})),
		}),

		async execute(_toolCallId, params, signal, _onUpdate, ctx): Promise<ToolResult> {
			const code = params.code as string | undefined;
			const file = params.file as string | undefined;
			if (!code && !file) {
				return {
					content: [{ type: "text", text: "Error: provide either `code` or `file`." }],
					details: { error: "Missing `code` or `file`" },
				};
			}
			if (code && file) {
				return {
					content: [{ type: "text", text: "Error: provide `code` OR `file`, not both." }],
					details: { error: "`code` and `file` are mutually exclusive" },
				};
			}

			const args = ["fix", code ?? (file as string)];
			if (typeof params.max_iters === "number") args.push("--max-iters", String(params.max_iters));
			if (params.safe_only === true) args.push("--safe-only");

			try {
				const result = await runIlo(args, undefined, signal, ctx.cwd);
				let parsed: Record<string, unknown> = {};
				try {
					parsed = JSON.parse(result.stdout.trim()) as Record<string, unknown>;
				} catch {
					return {
						content: [{ type: "text", text: result.stderr || `ilo fix failed (exit ${result.exitCode}).` }],
						details: { error: "unparseable fixer output", exitCode: result.exitCode },
					};
				}
				const fixed = parsed.fixed_source as string | null | undefined;
				const remaining = parsed.remaining_fixable as number | undefined;
				const applied = parsed.applied as number | undefined;
				const text = fixed
					? `Applied ${applied ?? 0} repair(s).\n\n${fixed}${
						remaining ? `\n\n${remaining} diagnostic(s) still need a manual fix.` : ""
					}`
					: `No mechanical repair applied.${
						remaining ? ` ${remaining} diagnostic(s) need a manual fix.` : " The program verifies."
					}`;
				return {
					content: [{ type: "text", text }],
					details: { ...parsed },
				};
			} catch (err) {
				const message = describeIloError(err);
				return {
					content: [{ type: "text", text: `Error: ${message}` }],
					details: { error: message },
				};
			}
		},
	});

	pi.registerTool({
		name: "ilo_run",
		label: "Run ilo",
		description:
			"Run an ilo program. Pass `code` for an inline source string, or `file` for a .ilo path. `func` runs a specific function; `args` are forwarded to it. Returns stdout, stderr, and the exit code. Prefer this over shelling out to `ilo` from inside pi: it is faster, structured, and skips the per-call permission prompt.",
		parameters: Type.Object({
			code: Type.Optional(Type.String({
				description: "Inline ilo source. Mutually exclusive with `file`.",
			})),
			file: Type.Optional(Type.String({
				description: "Path to a .ilo file. Mutually exclusive with `code`.",
			})),
			func: Type.Optional(Type.String({
				description: "Name of a function to invoke instead of running top-level code.",
			})),
			args: Type.Optional(Type.Array(Type.String(), {
				description: "Arguments passed to the program (or to `func` if set).",
			})),
		}),

		async execute(_toolCallId, params, signal, _onUpdate, ctx): Promise<ToolResult> {
			const code = params.code as string | undefined;
			const file = params.file as string | undefined;
			const func = params.func as string | undefined;
			const args = (params.args as string[] | undefined) ?? [];

			if (!code && !file) {
				return {
					content: [{ type: "text", text: "Error: provide either `code` or `file`." }],
					details: { error: "Missing `code` or `file`" },
				};
			}
			if (code && file) {
				return {
					content: [{ type: "text", text: "Error: provide `code` OR `file`, not both." }],
					details: { error: "`code` and `file` are mutually exclusive" },
				};
			}

			const cliArgs: string[] = [];
			cliArgs.push(code ?? (file as string));
			if (func) cliArgs.push(func);
			cliArgs.push(...args);

			try {
				const result = await runIlo(cliArgs, undefined, signal, ctx.cwd);
				const text = [
					result.stdout ? `stdout:\n${result.stdout}` : "stdout: (empty)",
					result.stderr ? `stderr:\n${readableDiagnostics(result.stderr)}` : "",
					`exit: ${result.exitCode}`,
				].filter(Boolean).join("\n\n");

				return {
					content: [{ type: "text", text }],
					details: { ...result },
				};
			} catch (err) {
				const message = describeIloError(err);
				return {
					content: [{ type: "text", text: `Error: ${message}` }],
					details: { error: message },
				};
			}
		},
	});

	pi.registerTool({
		name: "ilo_repl",
		label: "ilo REPL session",
		description:
			"Hold an interactive `ilo serv` session for iterative work. Call with `command: 'start'` to spawn the session, `command: 'send'` with `program` to run an ilo program in the running session, and `command: 'stop'` to terminate it. The session auto-closes after 5 minutes of inactivity. Uses ilo's documented `serv` JSON protocol: each `send` is one request, one response.",
		parameters: Type.Object({
			command: Type.Union([
				Type.Literal("start"),
				Type.Literal("send"),
				Type.Literal("stop"),
				Type.Literal("status"),
			], { description: "Lifecycle action for the REPL session." }),
			program: Type.Optional(Type.String({
				description: "ilo source to run (required when command is 'send').",
			})),
			func: Type.Optional(Type.String({
				description: "Function to invoke inside `program`.",
			})),
			args: Type.Optional(Type.Array(Type.String(), {
				description: "Arguments forwarded to the program or function.",
			})),
		}),

		async execute(_toolCallId, params, signal, _onUpdate, ctx): Promise<ToolResult> {
			const command = params.command as string;

			if (command === "start") {
				try {
					startRepl(ctx.cwd);
					return {
						content: [{ type: "text", text: "ilo serv session started." }],
						details: { running: true },
					};
				} catch (err) {
					const message = describeIloError(err);
					return {
						content: [{ type: "text", text: `Error: ${message}` }],
						details: { error: message },
					};
				}
			}

			if (command === "stop") {
				const wasRunning = repl !== null;
				stopRepl();
				return {
					content: [{ type: "text", text: wasRunning ? "Session stopped." : "No session was running." }],
					details: { running: false },
				};
			}

			if (command === "status") {
				return {
					content: [{ type: "text", text: repl ? "Session is running." : "No session." }],
					details: { running: repl !== null },
				};
			}

			if (command === "send") {
				const program = params.program as string | undefined;
				if (!program) {
					return {
						content: [{ type: "text", text: "Error: `program` is required when command is 'send'." }],
						details: { error: "Missing `program`" },
					};
				}
				const payload: Record<string, unknown> = { program };
				if (params.func) payload.func = params.func;
				if (params.args) payload.args = params.args;

				try {
					const responseLine = await sendToRepl(payload, signal, ctx.cwd);
					return {
						content: [{ type: "text", text: responseLine }],
						details: { response: responseLine },
					};
				} catch (err) {
					const message = describeIloError(err);
					return {
						content: [{ type: "text", text: `Error: ${message}` }],
						details: { error: message },
					};
				}
			}

			return {
				content: [{ type: "text", text: `Unknown command: ${command}` }],
				details: { error: "unknown command" },
			};
		},
	});
}
