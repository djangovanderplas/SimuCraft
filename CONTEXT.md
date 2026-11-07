# SimuCraft glossary

Words with one meaning in this project. Use them as written here.

| Term | Meaning |
| --- | --- |
| Model | One `.scmodel` file: parameters, blocks, nets, signals. A subsystem file is also a model. |
| Block | One item in a model, with a stable ID (`b_…`) and a unique name in its model. |
| Block type | What a block is, `library.Type` (for example `core.Gain`). Defined by a `BlockType` in Rust. |
| Port | A signal input or output of a block, written `<block>.<port>`. |
| Connector | A physical (acausal) port. Connectors join through nets. |
| Net | A set of connectors that are joined (`n_…`). |
| Signal flow | Causal modeling: blocks compute outputs from inputs. |
| Acausal / physical network | Equation-based modeling with across and through variables. |
| Stable ID | Prefix + `_` + 12 Crockford base32 characters. Never changes. |
| Path | Name-based address: `/` between subsystem levels, `.` for a member (`guidance/pid.Kp`). |
| Compiled model | The flat form the compiler writes and the runtime runs (`ir.rs`). |
| Model service | The process that holds project state and runs edit commands. It runs no user code. |
| Worker | A process that runs simulations, including user Python. |
| Model API | The JSON-RPC commands that the editor, Python and MCP all use. |
| Experiment | An `.scexp` file: a batch of runs (sweep, Monte Carlo). |
| Design study | A `.scdesign` file: operating points, tunable forms, loops, requirements. |
| Diagnostic | A reported problem: `{code, severity, message, file, path, id, span, fix}`. |
| Lane | A group of work packages that owns its own directories (A to F). |
| Gate | A point where all lanes integrate: G0 to G3. |
| Fake | A small working stand-in for a contract, in `simucraft_contracts::fake`. |
