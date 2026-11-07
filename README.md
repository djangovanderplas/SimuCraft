# SimuCraft

Open-source block-diagram modeling and simulation: signal flow and physical
(acausal) networks, with a Python API and an MCP server for AI agents. Every
model is a text file that diffs and merges in Git.

> Status: pre-alpha. Phase 0 (formats and contracts) is done; the engine is
> being built. Nothing here is usable for real work yet.

## Layout

| Path | What |
| --- | --- |
| `crates/simucraft-contracts` | Interfaces between components: diagnostics, block contract, compiled model, Model API |
| `crates/simucraft-format` | Project files: schemas, checks, format-preserving writer |
| `python/` | The `simucraft` Python package (Rust extension built with maturin) |
| `examples/` | Example projects; every file must pass `simucraft.check` |
| `docs/` | Spec, work packages, decisions (`docs/adr/`), research |
| `spikes/` | Throwaway experiments behind decisions; not part of the build |

## Develop

Needs Rust (stable) and [uv](https://docs.astral.sh/uv/).

```sh
scripts/check.sh        # format, lint, Rust tests, Python wheel, Python tests
scripts/check.sh --fix  # same, after auto-fixing format and lints
```

## License

Apache-2.0. See [LICENSE](LICENSE).
