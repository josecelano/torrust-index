#!/usr/bin/env python3
"""Generate docs/snapshots/svg/storyboard.svg.

Produces a single, fully self-contained SVG that shows the G-Tree (left
column) and V-Tree (right column) side-by-side for every snapshot step,
stacked vertically so you can scroll through the whole evolution at once.

Each diagram is embedded as a base64 data-URI <image> element, so the output
file has no external dependencies and opens in any SVG-capable viewer or
browser.

Layout per row
--------------
  ┌─────────────────┬──────────────────────────────────┬──────────────────────────────────┐
  │  Step label     │  G-Tree                          │  V-Tree                          │
  └─────────────────┴──────────────────────────────────┴──────────────────────────────────┘

Usage
-----
  ./docs/snapshots/render-storyboard.py          # from repo root
  cd docs/snapshots && ./render-storyboard.py    # from inside the folder

Requirements
------------
  • Python ≥ 3.8  (standard library only)
  • Graphviz  —  https://graphviz.org/
      apt:  sudo apt install graphviz
      brew: brew install graphviz
"""

from __future__ import annotations

import base64
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

# ── constants ────────────────────────────────────────────────────────────────

PADDING = 24          # px gap between columns / rows
LABEL_WIDTH = 180     # px width of the step-label column
LABEL_FONT = 13       # px font size for step labels
ROW_HEADER_H = 28     # px height of the column-header row at the top
PT_TO_PX = 96 / 72   # Graphviz uses pt; convert to CSS px (96 dpi)

# ── helpers ──────────────────────────────────────────────────────────────────


def svg_natural_size(svg_path: Path) -> tuple[float, float]:
    """Return (width_px, height_px) from a Graphviz-generated SVG file."""
    tree = ET.parse(svg_path)
    root = tree.getroot()

    # Prefer the viewBox (most reliable for Graphviz output).
    vb = root.get("viewBox")
    if vb:
        parts = vb.split()
        return float(parts[2]) * PT_TO_PX, float(parts[3]) * PT_TO_PX

    def _px(attr: str) -> float:
        val = root.get(attr, "200pt").rstrip("pt")
        return float(val) * PT_TO_PX

    return _px("width"), _px("height")


def svg_to_data_uri(svg_path: Path) -> str:
    """Encode an SVG file as a base64 data URI."""
    b64 = base64.b64encode(svg_path.read_bytes()).decode()
    return f"data:image/svg+xml;base64,{b64}"


def render_dot(dot_path: Path, svg_path: Path) -> None:
    """Render a single .dot file to SVG using Graphviz."""
    subprocess.run(
        ["dot", "-Tsvg", str(dot_path), "-o", str(svg_path)],
        check=True,
        capture_output=True,
    )


def step_label_from_filename(stem: str) -> str:
    """Turn e.g. 'step_03_obs_coord_2_value_1_SPLIT_①_G0_gtree' into
    a two-line label:  'Step 03\nobs coord=2 value=1 → SPLIT ① G0'."""
    # Strip trailing _gtree / _vtree
    stem = re.sub(r"_(gtree|vtree)$", "", stem)
    # Extract step number and the rest
    m = re.match(r"step_(\d+)_(.+)", stem)
    if not m:
        return stem
    num, rest = m.group(1), m.group(2)
    # Restore some punctuation from the sanitised filename
    rest = rest.replace("_SPLIT_", " → SPLIT ").replace("_G0", " G0")
    rest = rest.replace("_G1", " G1").replace("_G2", " G2")
    rest = rest.replace("_", " ").strip()
    return f"Step {num}\n{rest}"


# ── main ─────────────────────────────────────────────────────────────────────


def main() -> None:
    script_dir = Path(__file__).parent.resolve()
    svg_dir = script_dir / "svg"
    svg_dir.mkdir(exist_ok=True)

    # ── 1. Check for Graphviz ─────────────────────────────────────────────
    if not shutil.which("dot"):
        sys.exit(
            "ERROR: 'dot' (Graphviz) is not installed or not in PATH.\n"
            "       apt: sudo apt install graphviz  |  brew: brew install graphviz"
        )

    # ── 2. Collect and pair .dot files by step ────────────────────────────
    dot_files = sorted(script_dir.glob("*.dot"))
    if not dot_files:
        sys.exit(
            "No .dot files found in " + str(script_dir) + ".\n"
            "Run 'cargo run --example tree_snapshot' first."
        )

    # steps: ordered list of {num, label_lines, gtree_dot, vtree_dot}
    step_map: dict[int, dict] = {}
    for dot_file in dot_files:
        m = re.match(r"(step_(\d+)_.+)_(gtree|vtree)\.dot$", dot_file.name)
        if not m:
            continue
        _, num_str, kind = m.group(1), m.group(2), m.group(3)
        num = int(num_str)
        entry = step_map.setdefault(num, {"num": num, "gtree": None, "vtree": None})
        entry[kind] = dot_file

    steps = sorted(step_map.values(), key=lambda s: s["num"])
    if not steps:
        sys.exit("No step_NN_..._gtree.dot / _vtree.dot pairs found.")

    # ── 3. Render each .dot → .svg ────────────────────────────────────────
    print(f"Rendering {len(dot_files)} .dot files → {svg_dir}/")
    for step in steps:
        for kind in ("gtree", "vtree"):
            dot = step[kind]
            if dot is None:
                continue
            svg = svg_dir / (dot.stem + ".svg")
            render_dot(dot, svg)
            step[f"{kind}_svg"] = svg
            print(f"  {svg.name}")

    # ── 4. Measure every SVG ──────────────────────────────────────────────
    for step in steps:
        for kind in ("gtree", "vtree"):
            svg = step.get(f"{kind}_svg")
            if svg:
                w, h = svg_natural_size(svg)
                step[f"{kind}_w"] = w
                step[f"{kind}_h"] = h

    col_g_w = max((s.get("gtree_w", 0) for s in steps), default=200.0)
    col_v_w = max((s.get("vtree_w", 0) for s in steps), default=200.0)

    row_heights = []
    for step in steps:
        gh = step.get("gtree_h", 0)
        vh = step.get("vtree_h", 0)
        row_heights.append(max(gh, vh, 60.0))

    total_w = LABEL_WIDTH + PADDING + col_g_w + PADDING + col_v_w + PADDING
    total_h = (
        ROW_HEADER_H
        + PADDING
        + sum(row_heights)
        + PADDING * len(row_heights)
        + PADDING
    )

    # ── 5. Build the storyboard SVG ───────────────────────────────────────
    print(f"\nAssembling storyboard  ({total_w:.0f} × {total_h:.0f} px) …")

    lines: list[str] = []
    W = f"{total_w:.2f}"
    H = f"{total_h:.2f}"

    lines.append(f'<svg xmlns="http://www.w3.org/2000/svg"')
    lines.append(f'     xmlns:xlink="http://www.w3.org/1999/xlink"')
    lines.append(f'     width="{W}" height="{H}" viewBox="0 0 {W} {H}">')
    lines.append(f'  <rect width="{W}" height="{H}" fill="#f8f8f8"/>')

    # Column headers
    x_g = LABEL_WIDTH + PADDING
    x_v = x_g + col_g_w + PADDING
    y_hdr = ROW_HEADER_H * 0.7

    header_style = (
        f'font-family="Courier New, monospace" font-size="14" '
        f'font-weight="bold" fill="#333"'
    )
    lines.append(
        f'  <text x="{x_g + col_g_w / 2:.1f}" y="{y_hdr:.1f}" '
        f'text-anchor="middle" {header_style}>G-Tree</text>'
    )
    lines.append(
        f'  <text x="{x_v + col_v_w / 2:.1f}" y="{y_hdr:.1f}" '
        f'text-anchor="middle" {header_style}>V-Tree</text>'
    )

    # Separator line under header
    lines.append(
        f'  <line x1="0" y1="{ROW_HEADER_H}" x2="{W}" y2="{ROW_HEADER_H}" '
        f'stroke="#ccc" stroke-width="1"/>'
    )

    # Rows
    y_cursor = ROW_HEADER_H + PADDING
    for i, step in enumerate(steps):
        row_h = row_heights[i]
        num = step["num"]

        # Alternating background stripe
        stripe_fill = "#ffffff" if i % 2 == 0 else "#f0f4ff"
        lines.append(
            f'  <rect x="0" y="{y_cursor - PADDING / 2:.1f}" '
            f'width="{W}" height="{row_h + PADDING:.1f}" '
            f'fill="{stripe_fill}"/>'
        )

        # Step label (two lines: "Step NN" + description)
        label = step_label_from_filename(
            step["gtree"].stem if step.get("gtree") else step["vtree"].stem
        )
        label_parts = label.split("\n", 1)
        label_style = (
            'font-family="Courier New, monospace" '
            f'font-size="{LABEL_FONT}" fill="#222"'
        )
        y_label_top = y_cursor + row_h / 2 - (LABEL_FONT if len(label_parts) > 1 else 0)
        lines.append(
            f'  <text x="{LABEL_WIDTH / 2:.1f}" y="{y_label_top:.1f}" '
            f'text-anchor="middle" font-weight="bold" {label_style}>'
            f'{label_parts[0]}</text>'
        )
        if len(label_parts) > 1:
            lines.append(
                f'  <text x="{LABEL_WIDTH / 2:.1f}" y="{y_label_top + LABEL_FONT + 4:.1f}" '
                f'text-anchor="middle" {label_style}>'
                f'{label_parts[1]}</text>'
            )

        # G-Tree image
        if step.get("gtree_svg"):
            uri = svg_to_data_uri(step["gtree_svg"])
            lines.append(
                f'  <image x="{x_g:.1f}" y="{y_cursor:.1f}" '
                f'width="{step["gtree_w"]:.1f}" height="{step["gtree_h"]:.1f}" '
                f'href="{uri}"/>'
            )

        # V-Tree image
        if step.get("vtree_svg"):
            uri = svg_to_data_uri(step["vtree_svg"])
            lines.append(
                f'  <image x="{x_v:.1f}" y="{y_cursor:.1f}" '
                f'width="{step["vtree_w"]:.1f}" height="{step["vtree_h"]:.1f}" '
                f'href="{uri}"/>'
            )

        # Separator line below row
        y_sep = y_cursor + row_h + PADDING / 2
        lines.append(
            f'  <line x1="0" y1="{y_sep:.1f}" x2="{W}" y2="{y_sep:.1f}" '
            f'stroke="#ddd" stroke-width="0.5"/>'
        )

        y_cursor += row_h + PADDING

    # Vertical column separators
    for x_sep in (LABEL_WIDTH, x_g + col_g_w):
        lines.append(
            f'  <line x1="{x_sep:.1f}" y1="0" x2="{x_sep:.1f}" y2="{H}" '
            f'stroke="#ccc" stroke-width="1"/>'
        )

    lines.append("</svg>")

    out_path = svg_dir / "storyboard.svg"
    out_path.write_text("\n".join(lines), encoding="utf-8")
    size_kb = out_path.stat().st_size / 1024
    print(f"\nWrote {out_path}  ({size_kb:.0f} KB)")
    print("Open with:")
    print(f"  xdg-open {out_path}   # Linux")
    print(f"  open     {out_path}   # macOS")


if __name__ == "__main__":
    main()
