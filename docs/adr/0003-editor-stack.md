# ADR 0003: Editor stack

- Status: accepted
- Date: 2026-11-08
- Decided by: the owner
- Work packages: WP1.7, WP3.1, WP3.2, WP3.3
- Spec: Section 7 ("Model API"), requirements U1 and U2

## Decision

1. **The desktop shell is Tauri.** It is not Electron.
2. **The UI is React with React Flow (`@xyflow/react`).** React Flow draws the
   canvas: blocks are nodes, wires are edges, and pan, zoom and selection come
   from the library.
3. **The model service runs as a Tauri sidecar on stdio.** The editor starts
   `simucraft serve` and sends JSON-RPC 2.0 requests, one per line, on its
   stdin. It reads the responses on its stdout. This is the transport that
   WP1.4 made.

## Reasons

- Tauri uses the webview of the operating system. The installer and the
  memory use are much smaller than with Electron, which ships a full Chromium.
- The Tauri host is Rust, as the SimuCraft core is. One Cargo workspace builds
  the service and the shell, and the service binary goes into the bundle as a
  sidecar.
- React Flow gives the node-and-edge canvas that U1 needs. We do not write our
  own canvas, hit testing or viewport code.
- stdio needs no port, no socket file and no access control. Only the editor
  that started the service can send commands to it.

## Consequences

- **Webviews are not the same on all systems** (WebView2 on Windows, WKWebView
  on macOS, WebKitGTK on Linux). WP4.2 must test the editor on all three.
- **One service for each editor.** Spec Section 7 says a local socket, so that
  the editor, Python and MCP can share one service. With stdio, each client
  starts its own service. The clients still share the project files, and file
  sync (WP2.9) merges their edits. A socket transport can come in WP2.9 if a
  shared service is necessary. Then the editor changes only its transport.
- The editor is a client of the Model API. It holds no model state that the
  service does not have (spec Section 10).

## Alternatives

| Option | Why not |
| --- | --- |
| Electron | A larger download and more memory. The host is Node.js, not Rust. |
| A Rust-native UI (egui, Iced) | No mature node-editor library. We would write the canvas ourselves. |
| A local socket now | More code (port or socket file, access control) for no MVP gain. |
