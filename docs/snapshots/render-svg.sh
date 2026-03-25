#!/usr/bin/env bash
# Render all DOT snapshot files in this directory to SVG.
#
# Output is written to docs/snapshots/svg/ so that rendered files are kept
# separate from the source .dot files.
#
# Usage:
#   ./docs/snapshots/render-svg.sh          # from repo root
#   cd docs/snapshots && ./render-svg.sh    # from the snapshots folder
#
# Requirements: Graphviz (https://graphviz.org/)
#   apt:  sudo apt install graphviz
#   brew: brew install graphviz

set -euo pipefail

# Resolve the directory that contains this script, regardless of where it is
# called from.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="${SCRIPT_DIR}/svg"

if ! command -v dot &>/dev/null; then
    echo "ERROR: 'dot' (Graphviz) is not installed or not in PATH." >&2
    echo "       Install it with:  sudo apt install graphviz   or   brew install graphviz" >&2
    exit 1
fi

mkdir -p "${OUT_DIR}"

shopt -s nullglob
dot_files=("${SCRIPT_DIR}"/*.dot)

if [[ ${#dot_files[@]} -eq 0 ]]; then
    echo "No .dot files found in ${SCRIPT_DIR}."
    echo "Run 'cargo run --example tree_snapshot' first to generate them."
    exit 0
fi

echo "Rendering ${#dot_files[@]} .dot files → ${OUT_DIR}/"
for dot_file in "${dot_files[@]}"; do
    base="$(basename "${dot_file}" .dot)"
    svg_file="${OUT_DIR}/${base}.svg"
    dot -Tsvg "${dot_file}" -o "${svg_file}"
    echo "  ${base}.svg"
done

echo ""
echo "Done. Open the SVGs with:"
echo "  xdg-open ${OUT_DIR}/step_00_initial-empty_gtree.svg   # Linux"
echo "  open     ${OUT_DIR}/step_00_initial-empty_gtree.svg   # macOS"
