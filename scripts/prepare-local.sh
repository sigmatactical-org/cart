#!/usr/bin/env bash
# Link theme and patch the git dependency for local/CI builds.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKSPACE="$(cd "$ROOT/.." && pwd)"
cd "$ROOT"

THEME_PATH=""
for candidate in "$WORKSPACE/theme" "$WORKSPACE/../sigma/theme"; do
  if [[ -d "$candidate/ts" ]]; then
    THEME_PATH="$(cd "$candidate" && pwd)"
    break
  fi
done
if [[ -z "$THEME_PATH" ]]; then
  THEME_PATH="$WORKSPACE/theme"
  git clone --depth 1 https://github.com/sigmatactical-org/sigma-theme.git "$THEME_PATH"
fi

mkdir -p .cargo
cat > .cargo/config.toml <<EOF
[patch."https://github.com/sigmatactical-org/sigma-theme.git"]
sigma-theme = { path = "$THEME_PATH" }
EOF

cat > askama.toml <<EOF
[general]
dirs = ["templates", "$THEME_PATH/assets/templates"]
EOF

(cd "$THEME_PATH/ts" && npm ci && npm run check && npm run build)

echo "sigma-theme ($THEME_PATH) ready for cargo build."
