import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "typebox";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve as resolvePath } from "node:path";
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
	buffer: string;
	idleTimer: NodeJS.Timeout | null;
	pending: PendingRequest[];
}

let repl: ReplState | null = null;

function resolveIloBinary(): string {
	const fromEnv = process.env.ILO_BIN;
	if (fromEnv && existsSync(fromEnv)) return fromEnv;

	const extDir = dirname(fileURLToPath(import.meta.url));
	const bundled = resolvePath(extDir, "..", "node_modules", ".bin", "ilo");
	if (existsSync(bundled)) return bundled;

	return "ilo";
}

function runIlo(args: string[], stdin?: string, signal?: AbortSignal): Promise<{
	stdout: string;
	stderr: string;
	exitCode: number | null;
}> {
	return new Promise((resolve, reject) => {
		const child = spawn(resolveIloBinary(), args, {
			stdio: ["pipe", "pipe", "pipe"],
		});

		let stdout = "";
		let stderr = "";

		child.stdout.on("data", (chunk) => {
			stdout += chunk.toString();
		});
		child.stderr.on("data", (chunk) => {
			stderr += chunk.toString();
		});

		child.on("error", reject);
		child.on("close", (exitCode) => {
			resolve({ stdout, stderr, exitCode });
		});

		if (signal) {
			signal.addEventListener("abort", () => {
				child.kill("SIGTERM");
			}, { once: true });
		}

		if (stdin !== undefined) {
			child.stdin.write(stdin);
		}
		child.stdin.end();
	});
}

function resetReplIdleTimer() {
	if (!repl) return;
	if (repl.idleTimer) clearTimeout(repl.idleTimer);
	repl.idleTimer = setTimeout(() => {
		stopRepl();
	}, REPL_IDLE_TIMEOUT_MS);
}

function stopRepl() {
	if (!repl) return;
	if (repl.idleTimer) clearTimeout(repl.idleTimer);
	for (const pending of repl.pending) {
		pending.reject(new Error("REPL session stopped before response arrived"));
	}
	try {
		repl.process.kill("SIGTERM");
	} catch {
		// ignore
	}
	repl = null;
}

function startRepl(): ReplState {
	if (repl) return repl;

	const proc = spawn(resolveIloBinary(), ["serv"], {
		stdio: ["pipe", "pipe", "pipe"],
	});

	const state: ReplState = {
		process: proc,
		buffer: "",
		idleTimer: null,
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
			const pending = state.pending.shift();
			if (pending) {
				pending.resolve(line);
			} else if (line.length > 0) {
				process.stderr.write(`pi-ilo-lang: orphan response from ilo serv: ${line}\n`);
			}
			newlineIndex = state.buffer.indexOf("\n");
		}
	});

	proc.stdin.on("error", (err) => {
		const head = state.pending.shift();
		if (head) head.reject(err);
	});

	proc.on("close", () => {
		for (const pending of state.pending) {
			pending.reject(new Error("ilo serv exited"));
		}
		state.pending = [];
		if (repl === state) repl = null;
	});

	proc.on("error", (err) => {
		for (const pending of state.pending) {
			pending.reject(err);
		}
		state.pending = [];
		if (repl === state) repl = null;
	});

	resetReplIdleTimer();
	return state;
}

function sendToRepl(payload: object, signal?: AbortSignal): Promise<string> {
	const state = repl ?? startRepl();
	resetReplIdleTimer();

	return new Promise<string>((resolve, reject) => {
		if (state.process.exitCode !== null || !state.process.stdin.writable) {
			reject(new Error("ilo serv session is not writable"));
			return;
		}

		const timer = setTimeout(() => {
			const index = state.pending.indexOf(entry);
			if (index !== -1) state.pending.splice(index, 1);
			reject(new Error("ilo serv request timed out"));
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
			const index = state.pending.indexOf(entry);
			if (index !== -1) state.pending.splice(index, 1);
			entry.reject(new Error("ilo serv request aborted"));
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

export default function (pi: ExtensionAPI) {
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

		async execute(_toolCallId, params, signal): Promise<ToolResult> {
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
				const result = await runIlo(cliArgs, undefined, signal);
				const text = [
					result.stdout ? `stdout:\n${result.stdout}` : "stdout: (empty)",
					result.stderr ? `stderr:\n${result.stderr}` : "",
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

		async execute(_toolCallId, params, signal): Promise<ToolResult> {
			const command = params.command as string;

			if (command === "start") {
				try {
					startRepl();
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
					const responseLine = await sendToRepl(payload, signal);
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
