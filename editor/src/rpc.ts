// Model API client. In Tauri the shell pipes requests to the service sidecar
// (ADR 0003); in a plain browser the Vite dev server does (vite.config.ts).

import { invoke, isTauri } from "@tauri-apps/api/core";

export interface Diagnostic {
  code: string;
  severity: string;
  message: string;
  path?: string;
}

export class CommandError extends Error {
  constructor(
    message: string,
    readonly diagnostics: Diagnostic[],
  ) {
    super(message);
  }
}

let next = 0;

export async function rpc<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  const line = JSON.stringify({ jsonrpc: "2.0", id: ++next, method, params });
  const reply = isTauri()
    ? await invoke<string>("rpc", { line })
    : await (await fetch("/rpc", { method: "POST", body: line })).text();
  const msg = JSON.parse(reply);
  if (msg.error) throw new CommandError(msg.error.message, msg.error.diagnostics ?? []);
  return msg.result as T;
}

/** The project folder to open at start: the editor's first argument, or `?project=` in a browser. */
export async function initialProject(): Promise<string> {
  if (isTauri()) return (await invoke<string | null>("initial_project")) ?? "";
  return new URLSearchParams(window.location.search).get("project") ?? "";
}
