//! The SimuCraft editor shell: one window with the viewer UI, and the model
//! service as a sidecar on stdio (ADR 0003). `simucraft-editor [PROJECT]`.

// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

/// The running model service.
struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Send one JSON-RPC request line to the service and return its response line.
#[tauri::command]
fn rpc(line: String, service: tauri::State<'_, Mutex<Sidecar>>) -> Result<String, String> {
    let mut s = service.lock().map_err(|_| "the model service connection is broken")?;
    writeln!(s.stdin, "{}", line.replace('\n', " "))
        .and_then(|_| s.stdin.flush())
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    match s.stdout.read_line(&mut reply) {
        Ok(0) => Err("the model service stopped".into()),
        Ok(_) => Ok(reply),
        Err(e) => Err(e.to_string()),
    }
}

/// The project folder from the command line, if any.
#[tauri::command]
fn initial_project() -> Option<String> {
    std::env::args().nth(1)
}

/// `SIMUCRAFT_SERVE`, else `simucraft-serve` next to this program, else the
/// workspace debug build (development), else the one on `PATH`.
fn service_program() -> PathBuf {
    let name = format!("simucraft-serve{}", std::env::consts::EXE_SUFFIX);
    if let Some(p) = std::env::var_os("SIMUCRAFT_SERVE") {
        return p.into();
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&name)));
    let workspace = cfg!(debug_assertions).then(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug")
            .join(&name)
    });
    beside
        .into_iter()
        .chain(workspace)
        .find(|p| p.exists())
        .unwrap_or(name.into())
}

fn main() {
    let program = service_program();
    let mut child = match Command::new(&program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot start the model service `{}`: {e}", program.display());
            eprintln!("build it with `cargo build -p simucraft-service`, or set SIMUCRAFT_SERVE to its path");
            std::process::exit(1);
        }
    };
    let sidecar = Sidecar {
        stdin: child.stdin.take().expect("piped stdin"),
        stdout: BufReader::new(child.stdout.take().expect("piped stdout")),
        child,
    };
    tauri::Builder::default()
        .manage(Mutex::new(sidecar))
        .invoke_handler(tauri::generate_handler![rpc, initial_project])
        .run(tauri::generate_context!())
        .expect("the Tauri app runs");
}
