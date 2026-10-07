#!/usr/bin/env python3
"""Compares two versions' memory scenarios from docs/performance-history.csv.

Draws the baseline-vs-candidate bar chart embedded in docs/PERFORMANCE.md and
prints the matching Markdown delta table, so both are regenerated from the CSV
rather than hand-edited.

Needs matplotlib (`pip install matplotlib`).

Usage:
    python scripts/perf-chart.py --baseline 2.0.0 --candidate 2.5.0
    python scripts/perf-chart.py --baseline 2.0.0 --candidate 2.5.0 --os linux --out docs/x.png
    python scripts/perf-chart.py --baseline 2.5.0 --candidate 2.6.0 --runs 2

For each version/OS/scenario it uses the newest row whose label is the
scenario name (legacy runs' "-recheck" rows count as the same scenario), so a
re-run supersedes an earlier one without deleting history. With --runs N it
averages the newest N rows instead, for scenarios too noisy to judge from one
run (idle private bytes can differ by ~100MB between runs of the same build).

Only rows from one kind of profile are compared (--profile): "scratch" rows
come from the throwaway profiles perf-memory-scenarios.ts runs in, "real" rows
from runs against the developer's own profile (everything before #1197). The
first three scenarios must exist for both versions; the others are drawn when
both versions have them (a build that predates WebDAV has no webdav rows).
"""

import argparse
import csv
import sys
import textwrap
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
# (CSV label, table name, chart label)
REQUIRED_SCENARIOS = [
    ("idle", "Idle", "Idle"),
    ("after-full-scan", "After full scan", "After full scan"),
    ("playback-eq-analyzer", "During playback (EQ + analyzer on)", "Playback\n(EQ + analyzer)"),
]
OPTIONAL_SCENARIOS = [
    ("initial-scan", "First scan (with artwork)", "First scan"),
    ("album-grid", "Albums grid scrolled (thumbnails)", "Albums grid"),
    ("webdav-sync", "WebDAV first sync", "WebDAV sync"),
    ("webdav-idle", "WebDAV idle", "WebDAV idle"),
    ("webdav-playback", "WebDAV playback (EQ + analyzer on)", "WebDAV\nplayback"),
    ("subsonic-sync", "Subsonic first sync", "Subsonic sync"),
    ("subsonic-idle", "Subsonic idle", "Subsonic idle"),
    ("subsonic-playback", "Subsonic playback (EQ + analyzer on)", "Subsonic\nplayback"),
]
METRICS = [("private_bytes_mb", "Private bytes"), ("working_set_mb", "Working set")]
PEAK_METRICS = [("peak_private_bytes_mb", "Peak private bytes"), ("peak_working_set_mb", "Peak working set")]

SURFACE = "#fcfcfb"
INK = "#1a1a19"
INK_2 = "#5f5e58"
GRID = "#e6e5e0"
BASE_COLOR = "#b4b2a9"  # recessive neutral for the reference version
NEW_COLOR = "#2a78d6"


def latest_rows(rows, version, os_name, profile, runs=1):
    known = {key for key, *_ in REQUIRED_SCENARIOS + OPTIONAL_SCENARIOS}
    picked = {}
    for r in rows:  # file order is chronological, so the newest rows come last
        if r["app_version"] != version or r["os"] != os_name or (r.get("profile") or "real") != profile:
            continue
        label = r["label"].removesuffix("-recheck")
        if label in known:
            picked.setdefault(label, []).append(r)
    short = [key for key, *_ in REQUIRED_SCENARIOS if len(picked.get(key, [])) < runs]
    if short:
        raise SystemExit(
            f"Fewer than {runs} {profile}-profile {os_name} row(s) for {version} scenario(s): {', '.join(short)}"
        )
    averaged = {}
    for label, found in picked.items():
        if len(found) < runs:
            continue
        newest = found[-runs:]
        averaged[label] = dict(newest[-1])
        for metric, _ in METRICS + PEAK_METRICS:
            values = [float(r[metric]) for r in newest if r.get(metric)]
            # Peaks are blank in rows older than #1197; average only a complete set.
            averaged[label][metric] = sum(values) / runs if len(values) == runs else None
    return averaged


def shared_scenarios(base, cand):
    """The required scenarios, then each optional one both versions have."""
    return REQUIRED_SCENARIOS + [s for s in OPTIONAL_SCENARIOS if s[0] in base and s[0] in cand]


def describe(rows):
    r = rows["idle"]
    tracks = f"{int(r['library_tracks']):,} tracks" if r["library_tracks"] else "unknown library size"
    return f"{r['timestamp'][:10]}, {tracks}"


def delta(b, c):
    if b is None or c is None:
        return "not recorded"
    d = c - b
    return f"{d:+.1f} MB ({d / b * 100:+.1f}%)".replace("-", "−")


def markdown(base, cand):
    metrics = METRICS + PEAK_METRICS
    lines = ["| Scenario | " + " | ".join(f"{name} Δ" for _, name in metrics) + " |",
             "| --- |" + " --- |" * len(metrics)]
    for key, name, _ in shared_scenarios(base, cand):
        cells = [delta(base[key][metric], cand[key][metric]) for metric, _ in metrics]
        lines.append(f"| {name} | " + " | ".join(cells) + " |")
    return "\n".join(lines)


def chart(base, cand, args, out):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    plt.rcParams.update({"font.size": 10, "text.color": INK, "axes.labelcolor": INK_2,
                         "xtick.color": INK_2, "ytick.color": INK_2})
    scenarios = shared_scenarios(base, cand)
    fig, axes = plt.subplots(1, 2, figsize=(max(11, 3.2 * len(scenarios)), 4.8), sharey=True, facecolor=SURFACE)
    labels = [chart_label for *_, chart_label in scenarios]
    top = max(float(rows[k][m]) for rows in (base, cand) for k, *_ in scenarios for m, _ in METRICS)
    w, gap = 0.36, 0.02
    for ax, (metric, title) in zip(axes, METRICS):
        ax.set_facecolor(SURFACE)
        for x, (key, *_) in enumerate(scenarios):
            b, c = float(base[key][metric]), float(cand[key][metric])
            for off, v, color in ((-w / 2 - gap / 2, b, BASE_COLOR), (w / 2 + gap / 2, c, NEW_COLOR)):
                ax.bar(x + off, v, w, color=color, linewidth=0)
                ax.text(x + off, v - top * 0.017, f"{v:.0f}", ha="center", va="top", fontsize=8.5,
                        color=INK if color == BASE_COLOR else "#ffffff")
            d = c - b
            ax.text(x + w / 2 + gap / 2, c + top * 0.017, f"{d:+.0f} MB\n({d / b * 100:+.1f}%)",
                    ha="center", va="bottom", fontsize=9, color=INK)
        ax.set_xlim(-0.6, len(scenarios) - 0.4)
        ax.set_ylim(0, top * 1.2)
        ax.set_xticks(range(len(scenarios)), labels)
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
    if args.runs > 1:
        note += f" Mean of the newest {args.runs} runs per version."
    windows = [rows["idle"]["window"] or "not recorded" for rows in (base, cand)]
    if windows[0] == windows[1]:
        note += f" Window {windows[0]}."
    else:
        note += f" {args.baseline} window {windows[0]}; {args.candidate} window {windows[1]}."
    fig.text(0.012, 0.015, textwrap.fill(note, 170), fontsize=8.5, color=INK_2, va="bottom")
    fig.tight_layout(rect=(0, 0.06, 1, 0.9))
    fig.savefig(out, dpi=150, facecolor=SURFACE)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--baseline", required=True)
    p.add_argument("--candidate", required=True)
    p.add_argument("--os", default="windows")
    p.add_argument("--csv", default=REPO_ROOT / "docs" / "performance-history.csv", type=Path)
    p.add_argument("--out", type=Path, help="default: docs/performance-<baseline>-vs-<candidate>.png")
    p.add_argument("--runs", type=int, default=1, help="average the newest N runs per version (default 1)")
    p.add_argument("--profile", default="scratch", choices=["scratch", "real"],
                   help="compare rows from throwaway profiles (default) or the real profile (history before #1197)")
    args = p.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")  # the delta table uses Δ/−, which cp1252 consoles can't encode

    rows = list(csv.DictReader(args.csv.open(encoding="utf8")))
    base = latest_rows(rows, args.baseline, args.os, args.profile, args.runs)
    cand = latest_rows(rows, args.candidate, args.os, args.profile, args.runs)

    out = args.out or REPO_ROOT / "docs" / f"performance-{args.baseline}-vs-{args.candidate}.png"
    chart(base, cand, args, out)
    print(f"Wrote {out}\n")
    print(markdown(base, cand))
    for name, rows_ in ((args.baseline, base), (args.candidate, cand)):
        for key, what in (("initial-scan", "first scan"), ("after-full-scan", "forced full scan"),
                          ("webdav-sync", "WebDAV sync"), ("subsonic-sync", "Subsonic sync")):
            row = rows_.get(key)
            if row and row["scan_seconds"]:
                tracks = int(row["library_tracks"] or rows_["idle"]["library_tracks"])
                print(f"\n{name}: {what} of {tracks:,} tracks took {row['scan_seconds']}s")


if __name__ == "__main__":
    main()
