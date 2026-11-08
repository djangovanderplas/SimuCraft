//! `simucraft-serve`: the model service on stdio, one JSON-RPC request per
//! line in and one response per line out. The editor runs it as a sidecar
//! (ADR 0003). `simucraft serve` in the Python package does the same.

use std::io::{BufRead, Write};

fn main() {
    let mut service = simucraft_service::Service::default();
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        // A closed stdout means the client is gone.
        if writeln!(out, "{}", service.handle_json(&line))
            .and_then(|_| out.flush())
            .is_err()
        {
            break;
        }
    }
}
