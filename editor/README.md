# SimuCraft editor

A read-only diagram viewer for now (WP1.7). The stack is Tauri, React and
React Flow (ADR 0003). The editor is a client of the model service: it gets
each model and its layout with `model.get`.

## Run it in a browser

1. Build the model service: `cargo build -p simucraft-service`.
2. In `editor/`, run `npm ci`, then `npm run dev`.
3. Open `http://localhost:5173/?project=/path/to/project`, for example the
   folder `examples/motor_demo`.

The Vite dev server starts `target/debug/simucraft-serve` and sends it the
requests. Set `SIMUCRAFT_SERVE` to use another binary.

## Run it as a desktop app

1. Build the model service: `cargo build -p simucraft-service`.
2. In `editor/`, run `npm run tauri dev`.

The project folder is the first argument of the app. The shell starts
`simucraft-serve` as a sidecar on stdio. It looks for the binary in
`SIMUCRAFT_SERVE`, next to the app, in `target/debug` (debug builds only),
and on `PATH`.

## Use it

- Blocks with a layout entry stay where the `.sclayout` file puts them. The
  viewer places all other blocks in columns by signal depth. A block with only
  physical connections goes after its neighbour on a net.
- Dashed lines are physical nets.
- Double-click a subsystem (blue border) to open its file. Use the path at the
  top to go back.

## Test

`npm run check` runs the typecheck and the tests. `scripts/check.sh` runs it too.
