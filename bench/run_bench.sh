#!/usr/bin/env bash
# Reproducible wall-clock comparison of Tier 0 and Tier 1 for every benchmark.
# Usage: bash bench/run_bench.sh [runs]
set -euo pipefail
cd "$(dirname "$0")/.."
RUNS=${1:-7}
BIN=./target/debug/sukti
if [ ! -x "$BIN" ]; then cargo build -p sukti-bin; fi
measure() { # label args...
  local label=$1; shift
  for i in $(seq 1 "$RUNS"); do
    local s e
    s=$(date +%s%N); "$@" >/dev/null 2>&1; e=$(date +%s%N)
    echo $(( (e-s)/1000000 ))
  done | sort -n | awk -v l="$label" '{a[NR]=$1} END{printf "%-22s median=%6d ms  runs=%s\n", l, a[int((NR+1)/2)], NR}'
}
echo "Tier 0 vs Tier 1 -- $(rustc --version), $(uname -m)"
for name in fib30 loop nested methods sieve; do
  measure "$name Tier 0" "$BIN" run "bench/$name.sukti"
  measure "$name Tier 1" "$BIN" --jit=on --jit-threshold=1 run "bench/$name.sukti"
done
