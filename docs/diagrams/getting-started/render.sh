#!/usr/bin/env bash
# Render every Mermaid source in mermaid/ to rendered/{name}.svg + .png.
# Tune a .mmd file, re-run this, and the images regenerate. No global
# install needed — mermaid-cli is fetched (and cached) via npx.
#
#   ./render.sh            # render all
#   ./render.sh 03         # render only files matching "03"
set -euo pipefail
cd "$(dirname "$0")"

MMDC=(npx -y @mermaid-js/mermaid-cli@latest)
THEME="neutral"          # neutral | default | dark | forest
filter="${1:-}"

mkdir -p rendered
shopt -s nullglob
for f in mermaid/*"$filter"*.mmd; do
  name="$(basename "$f" .mmd)"
  echo "→ $name"
  "${MMDC[@]}" -i "$f" -o "rendered/${name}.svg" -b transparent -t "$THEME"
  "${MMDC[@]}" -i "$f" -o "rendered/${name}.png" -b white       -t "$THEME" -s 2
done
echo "done — see rendered/"
