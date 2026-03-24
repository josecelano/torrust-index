#!/usr/bin/env bash
set -euo pipefail

DOCS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Render .puml files to SVG
plantuml -Djava.awt.headless=true -tsvg "$DOCS_DIR"/*.puml

# Post-process: responsive sizing, correct aspect ratio, white background
for f in "$DOCS_DIR"/*.svg; do
    sed -i \
        -e 's/ width="[0-9]*px"//g' \
        -e 's/ height="[0-9]*px"//g' \
        -e 's/style="width:[0-9]*px;height:[0-9]*px;"/style="max-width:100%;height:auto;background:#ffffff;"/' \
        -e 's/preserveAspectRatio="none"/preserveAspectRatio="xMidYMid meet"/' \
        "$f"
    echo "Generated: $f"
done
