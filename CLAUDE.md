# SimuCraft: instructions for coding agents

SimuCraft is an open-source Simulink alternative: a Rust core, a Python
package, an MCP server, and text project files that merge in Git.

## Read first

- `docs/spec.md`: what to build (requirements M1-S1, file formats, API).
- `docs/work-packages.md`: the work packages (WP), their dependencies and
  their "done when" tests.
- `docs/adr/`: decisions with their reasons. Do not reverse one without a new ADR.
- `CONTEXT.md`: the glossary. Use its words.
- `docs/handoff.md`: current state and what to do next.

## Commands

```sh
scripts/check.sh        # the one check: fmt, clippy -D warnings, Rust tests, Python wheel + pytest
scripts/check.sh --fix  # auto-fix format and lints first
```

There is no CI while the repository is private. `scripts/check.sh` must pass
before every commit you push.

## Where code goes (lanes)

Each lane owns its directories. A work package edits only its lane's
directories, plus tests and docs for what it changed.

| Lane | Directories |
| --- | --- |
| A engine | `crates/simucraft-compiler`, `crates/simucraft-runtime` |
| B blocks, conformance | `crates/simucraft-blocks`, `conformance/` |
| C API, experiments | `crates/simucraft-service`, `crates/simucraft-experiments`, `crates/simucraft-mcp`, `python/` |
| D files, expressions | `crates/simucraft-format`, `crates/simucraft-expr`, `crates/simucraft-diff` |
| E physical modeling | `crates/simucraft-acausal` |
| F editor | `editor/` |
| Shared contracts | `crates/simucraft-contracts` |

Create a crate only when its work package starts. Do not add empty crates.

## Contract changes

`crates/simucraft-contracts` is frozen (gate G0, ADR 0002). If a work package
needs a change there, make that change in its own small commit and pull
request first, explain why, and update the fixtures and fakes in the same
change. Never change a contract silently inside a feature branch.

## How to write code

The goal is the least code that meets the done-when test.

1. Think before you code. Read the spec section and the existing code. Name
   your assumptions. If two readings of the spec are possible, pick the
   simpler one and write it down in the pull request.
2. Write the done-when test first (red), then make it pass (green), then
   simplify. One vertical slice at a time.
3. Simplicity first. No speculative abstractions, no options nobody asked
   for, no "future-proofing". Prefer the standard library. Add a dependency
   only if it removes a lot of code or risk, and say why in the commit.
4. Surgical changes. Touch only what the task needs. Do not reformat or
   rename unrelated code.
5. `unsafe` is forbidden in the workspace. FFI, if ever needed, goes in its own
   crate with a written reason.
6. Errors that a user can cause are `Diagnostic`s with a code, never panics.
7. Comments say why, not what. Doc comments on every public item.

## How to write prose

Write docs, doc comments, commit messages and pull request text in ASD-STE100
Simplified Technical English: short sentences, one instruction per sentence,
active voice, simple words, one meaning per word.

## Branches, commits, pull requests

- One branch per work package: `wp/<id>-<slug>`, for example `wp/1.8-expr`.
  Branch from `main`. If you need code from an unmerged branch, branch from
  it and say so in the pull request.
- Never push to `main`. Never merge your own pull request: the owner reviews
  and merges.
- Pull request title: `WP1.8: Expression engine`. Body: what changed, the
  done-when test and how it passes, contract changes, what you did not do,
  risks.
- Small commits with messages that say why.

## Plugins (use them when they fit)

These plugins are enabled in `.claude/settings.json`. If one is missing, follow
the rules above, which hold the same ideas.

- `ponytail`: start every session with `/ponytail full`. Smallest complete
  change; standard library first.
- `andrej-karpathy-skills`: its guidelines are rules 1-4 above.
- `mattpocock-skills`: `tdd` for each work package; `diagnose` for hard bugs;
  `zoom-out` on unfamiliar code; `improve-codebase-architecture` at gates;
  `handoff` at the end of a session.
- `asd-ste100`: for all prose (see above).
- `i-have-adhd`: for messages to the owner: next action first, numbered steps,
  clear status.
