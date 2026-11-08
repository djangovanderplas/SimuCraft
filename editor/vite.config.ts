import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

/** Dev only: answer POST /rpc from a `simucraft-serve` child, so the viewer runs in a plain browser. */
function devService(): Plugin {
  let child: ChildProcessWithoutNullStreams | null = null;
  const waiting: ((line: string) => void)[] = [];
  const start = () => {
    const bin = process.env.SIMUCRAFT_SERVE ?? fileURLToPath(new URL("../target/debug/simucraft-serve", import.meta.url));
    child = spawn(bin, [], { stdio: ["pipe", "pipe", "pipe"] });
    child.on("error", (e) => console.error(`simucraft-serve: ${e.message}. Run \`cargo build -p simucraft-service\`.`));
    child.on("exit", () => (child = null));
    child.stderr.pipe(process.stderr);
    createInterface({ input: child.stdout }).on("line", (line) => waiting.shift()?.(line));
    return child;
  };
  return {
    name: "simucraft-dev-service",
    apply: "serve",
    configureServer(server) {
      server.httpServer?.on("close", () => child?.kill());
      server.middlewares.use("/rpc", (req, res) => {
        let body = "";
        req.on("data", (c) => (body += c));
        req.on("end", () => {
          // The service answers in order, one line per request.
          waiting.push((line) => res.end(line));
          (child ?? start()).stdin.write(body.replace(/\n/g, " ") + "\n");
        });
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), devService()],
  // Tauri expects a fixed port and does not want the screen cleared.
  server: { port: 5173, strictPort: true },
  clearScreen: false,
});
