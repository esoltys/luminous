#!/usr/bin/env python3
"""Compares two versions' memory scenarios from docs/performance-history.csv.

Draws the baseline-vs-candidate bar chart embedded in docs/PERFORMANCE.md and
prints the matching Markdown delta table, so both are regenerated from the CSV
rather than hand-edited.

Needs matplotlib (`pip install matplotlib`).

Usage:
    python scripts/perf-chart.py --baseline 2.0.0 --candidate 2.5.0
    python scripts/perf-chart.py --baseline 2.0.0 --candidate 2.5.0 --os linux --out docs/x.png

For each version/OS/scenario it uses the newest row whose label is the
scenario name (legacy runs' "-recheck" rows count as the same scenario), so a
re-run supersedes an earlier one without deleting history.
"""

import argparse
import csv
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCENARIOS = [
    ("idle", "Idle"),
    ("after-full-scan", "After full scan"),
    ("playback-eq-analyzer", "During playback (EQ + analyzer on)"),
]
METRICS = [("private_bytes_mb", "Private bytes"), ("working_set_mb", "Working set")]

SURFACE = "#fcfcfb"
INK = "#1a1a19"
INK_2 = "#5f5e58"
GRID = "#e6e5e0"
BASE_COLOR = "#b4b2a9"  # recessive neutral for the reference version
NEW_COLOR = "#2a78d6"


def latest_rows(rows, version, os_name):
    picked = {}
    for r in rows:  # file order is chronological, so later rows win
        if r["app_version"] != version or r["os"] != os_name:
            continue
        label = r["label"].removesuffix("-recheck")
        if label in dict(SCENARIOS):
            picked[label] = r
    missing = [s for s, _ in SCENARIOS if s not in picked]
    if missing:
        raise SystemExit(f"No {os_name} rows for {version} scenario(s): {', '.join(missing)}")
    return picked


def describe(rows):
    r = rows["idle"]
    tracks = f"{int(r['library_tracks']):,} tracks" if r["library_tracks"] else "unknown library size"
    return f"{r['timestamp'][:10]}, {tracks}"


def markdown(base, cand):
    lines = ["| Scenario | Private bytes Δ | Working set Δ |", "| --- | --- | --- |"]
    for key, name in SCENARIOS:
        cells = []
        for metric, _ in METRICS:
            b, c = float(base[key][metric]), float(cand[key][metric])
            d = c - b
            cells.append(f"{d:+.1f} MB ({d / b * 100:+.1f}%)".replace("-", "−"))
        lines.append(f"| {name} | {cells[0]} | {cells[1]} |")
    return "\n".join(lines)


def chart(base, cand, args, out):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    plt.rcParams.update({"font.size": 10, "text.color": INK, "axes.labelcolor": INK_2,
                         "xtick.color": INK_2, "ytick.color": INK_2})
    fig, axes = plt.subplots(1, 2, figsize=(11, 4.8), sharey=True, facecolor=SURFACE)
    labels = ["Idle", "After full scan", "Playback\n(EQ + analyzer)"]
    top = max(float(rows[k][m]) for rows in (base, cand) for k, _ in SCENARIOS for m, _ in METRICS)
    w, gap = 0.36, 0.02
    for ax, (metric, title) in zip(axes, METRICS):
        ax.set_facecolor(SURFACE)
        for x, (key, _) in enumerate(SCENARIOS):
            b, c = float(base[key][metric]), float(cand[key][metric])
            for off, v, color in ((-w / 2 - gap / 2, b, BASE_COLOR), (w / 2 + gap / 2, c, NEW_COLOR)):
                ax.bar(x + off, v, w, color=color, linewidth=0)
                ax.text(x + off, v - top * 0.017, f"{v:.0f}", ha="center", va="top", fontsize=8.5,
                        color=INK if color == BASE_COLOR else "#ffffff")
            d = c - b
            ax.text(x + w / 2 + gap / 2, c + top * 0.017, f"{d:+.0f} MB\n({d / b * 100:+.1f}%)",
                    ha="center", va="bottom", fontsize=9, color=INK)
        ax.set_xlim(-0.6, len(SCENARIOS) - 0.4)
        ax.set_ylim(0, top * 1.2)
        ax.set_xticks(range(len(SCENARIOS)), labels)
        ax.set_title(f"{title} (MB)", loc="left", fontsize=11.5, color=INK, pad=10, fontweight="bold")
        ax.yaxis.grid(True, color=GRID, linewidth=0.8)
        ax.set_axisbelow(True)
        for side in ("top", "right", "left"):
            ax.spines[side].set_visible(False)
        ax.spines["bottom"].set_color(GRID)
        ax.tick_params(length=0)
    axes[0].set_ylabel("MB")

    handles = [plt.Rectangle((0, 0), 1, 1, color=BASE_COLOR), plt.Rectangle((0, 0), 1, 1, color=NEW_COLOR)]
    fig.legend(handles, [f"{args.baseline} baseline ({describe(base)})", f"{args.candidate} ({describe(cand)})"],
               loc="upper right", ncol=2, frameon=False, fontsize=9.5, bbox_to_anchor=(0.985, 0.935))
    os_title = {"windows": "Windows", "linux": "Linux"}.get(args.os, args.os)
    fig.suptitle(f"Luminous memory: {args.candidate} vs. {args.baseline} ({os_title}, release build)",
                 x=0.012, ha="left", fontsize=13, fontweight="bold", color=INK, y=0.985)
    note = "Total across the main process and its WebView child processes. Labels show the change from baseline."
    sizes = [f"{name} window {rows['idle']['window'] or 'not recorded'}"
             for name, rows in ((args.baseline, base), (args.candidate, cand))]
    note += f" {'; '.join(sizes)}."
    fig.text(0.012, 0.015, note, fontsize=8.5, color=INK_2)
    fig.tight_layout(rect=(0, 0.04, 1, 0.9))
    fig.savefig(out, dpi=150, facecolor=SURFACE)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--baseline", required=True)
    p.add_argument("--candidate", required=True)
    p.add_argument("--os", default="windows")
    p.add_argument("--csv", default=REPO_ROOT / "docs" / "performance-history.csv", type=Path)
    p.add_argument("--out", type=Path, help="default: docs/performance-<baseline>-vs-<candidate>.png")
    args = p.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")  # the delta table uses Δ/−, which cp1252 consoles can't encode

    rows = list(csv.DictReader(args.csv.open(encoding="utf8")))
    base = latest_rows(rows, args.baseline, args.os)
    cand = latest_rows(rows, args.candidate, args.os)

    out = args.out or REPO_ROOT / "docs" / f"performance-{args.baseline}-vs-{args.candidate}.png"
    chart(base, cand, args, out)
    print(f"Wrote {out}\n")
    print(markdown(base, cand))
    for name, rows_ in ((args.baseline, base), (args.candidate, cand)):
        secs = rows_["after-full-scan"]["scan_seconds"]
        if secs:
            print(f"\n{name}: forced full scan of {int(rows_['idle']['library_tracks']):,} tracks took {secs}s")


if __name__ == "__main__":
    main()
