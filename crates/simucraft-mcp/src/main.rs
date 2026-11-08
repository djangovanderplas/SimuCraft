//! `simucraft-mcp PROJECT`: the MCP server on stdio, one JSON-RPC message per line.

use std::io::{BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: simucraft-mcp PROJECT_FOLDER");
        return ExitCode::from(2);
    };
    let mut server = match simucraft_mcp::Server::open(Path::new(&root)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("simucraft-mcp: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if let Some(reply) = server.handle(&line) {
            // A closed stdout means the client is gone.
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
    ExitCode::SUCCESS
}
