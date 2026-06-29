#!/usr/bin/env bash
# Stage build/image/ (binary) for the COPY-only Dockerfile.
# Release builds run in .github/workflows/release.yml — not here.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

./scripts/prepare-local.sh
cargo build --release
./scripts/prepare-image-context.sh

echo "Staged: $ROOT/build/image/"
echo "Local image: docker build -f Dockerfile build/image"
