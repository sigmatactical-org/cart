#!/usr/bin/env bash
# Link theme and patch the git dependency for local/CI builds.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

THEME_PATH="theme"
if [[ -d ../theme/ts ]]; then
  THEME_PATH="../theme"
elif [[ ! -d theme/ts ]]; then
  git clone --depth 1 https://github.com/sigmatactical-org/sigma-theme.git theme
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
