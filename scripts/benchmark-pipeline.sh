#!/usr/bin/env bash
# Release-profile pipeline p99 benchmark (maturity §3.1: p99 < 0.5ms).
set -euo pipefail
cd "$(dirname "$0")/.."
echo "Running release pipeline benchmark..."
cargo test --release -p sdkwork-web-architecture-tests --test pipeline_benchmark
