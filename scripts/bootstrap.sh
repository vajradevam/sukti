#!/usr/bin/env bash
# Fresh-clone bootstrap: build the whole workspace and smoke-test the CLI.
set -euo pipefail
cd "$(dirname "$0")/.."
echo "[bootstrap] Rust: $(rustc --version)"
cargo build --workspace
echo "[bootstrap] checking lexicon data"
python3 tools/check_lexicon.py || true
echo "[bootstrap] running test suite"
cargo test --workspace
echo "[bootstrap] hello world"
printf 'vada("namaste, vishva!");\n' > /tmp/sukti-hello.sukti
cargo run -q -p sukti-bin -- run /tmp/sukti-hello.sukti
echo "[bootstrap] OK"
