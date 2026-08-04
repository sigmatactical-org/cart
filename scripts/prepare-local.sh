#!/usr/bin/env bash
# Link sibling Sigma checkouts so local edits to shared crates are picked up.
#
# Optional: a fresh clone of this repo builds with `cargo build` alone, because
# every shared crate is a pinned git dependency and sigma-theme ships its built
# assets. Run this when working in a tree where sigma-theme, sigma-pg, or another
# shared crate is checked out beside this repo and being edited.
#
# The linking itself lives in the platform repo so every service shares one
# implementation; without a platform checkout nearby there is nothing to link.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

helper=""
for candidate in \
  "$ROOT/../platform/scripts/link-local-crates.sh" \
  "$ROOT/../../platform/scripts/link-local-crates.sh"; do
  if [[ -f "$candidate" ]]; then
    helper="$candidate"
    break
  fi
done

if [[ -z "$helper" ]]; then
  echo "No platform checkout beside $ROOT: shared crates resolve from the pinned"
  echo "git revisions in Cargo.toml. Nothing to prepare; run cargo build."
  exit 0
fi

# shellcheck source=/dev/null
source "$helper"
link_local_crates "$ROOT"
