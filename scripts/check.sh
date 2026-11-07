#!/usr/bin/env bash
# Run every check the project has. CI will run this same script once the
# repository is public. Usage: scripts/check.sh [--fix]
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--fix" ]]; then
  cargo fmt --all
  cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged -q
fi

echo "== rust: format, lint, test"
cargo fmt --all --check
cargo clippy --workspace --all-targets -q -- -D warnings
cargo test --workspace -q

echo "== python: build wheel into .venv, test"
[[ -d .venv ]] || uv venv -q .venv
uv pip install -q --python .venv maturin pytest
.venv/bin/maturin develop -q -m python/Cargo.toml --uv
.venv/bin/pytest -q python/tests

echo "== all checks passed"
