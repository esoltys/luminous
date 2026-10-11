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
    python scripts/perf-chart.py --trend --runs 2    # every recorded version, as lines

For each version/OS/scenario it uses the newest row whose label is the
scenario name (legacy runs' "-recheck" rows count as the same scenario), so a
re-run supersedes an earlier one without deleting history. With --runs N it
averages the newest N rows instead, for scenarios too noisy to judge from one
run (idle private bytes can differ by ~100MB between runs of the same build).

--trend instead draws every recorded version of one OS as a line per scenario, with the "real" and
"scratch" profile histories as separate series (a version measured in both, such as 2.6.0, shows the
offset between the two kinds), so a trend is read within a series, never across the break.

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


PROFILE_STYLE = {  # recessive for the pre-#1197 history, strong for the controlled runs
    "real": dict(color=BASE_COLOR, linestyle="--", markerfacecolor=SURFACE, label="real profile (before #1197)"),
    "scratch": dict(color=NEW_COLOR, linestyle="-", markerfacecolor=NEW_COLOR, label="scratch profile"),
}


def version_key(v):
    return tuple(int(part) for part in v.split("."))


def trend_series(rows, os_name, runs):
    """{profile: {version: scenarios}} for every version/profile with a full idle, scan and playback set."""
    series = {}
    for profile in PROFILE_STYLE:
        versions = {r["app_version"] for r in rows if r["os"] == os_name and (r.get("profile") or "real") == profile}
        for v in sorted(versions, key=version_key):
            have = sum(1 for r in rows if r["app_version"] == v and r["os"] == os_name
                       and (r.get("profile") or "real") == profile and r["label"].removesuffix("-recheck") == "idle")
            if have:
                series.setdefault(profile, {})[v] = latest_rows(rows, v, os_name, profile, min(runs, have))
    return series


def trend_chart(series, args, out):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    plt.rcParams.update({"font.size": 10, "text.color": INK, "axes.labelcolor": INK_2,
                         "xtick.color": INK_2, "ytick.color": INK_2})
    versions = sorted({v for by_version in series.values() for v in by_version}, key=version_key)
    scenarios = [s for s in REQUIRED_SCENARIOS + OPTIONAL_SCENARIOS
                 if any(sum(1 for rows in by_v.values() if s[0] in rows) >= 2 for by_v in series.values())]
    fig, axes = plt.subplots(len(METRICS), len(scenarios), figsize=(3.4 * len(scenarios), 7), sharex=True,
                             sharey="row", facecolor=SURFACE, squeeze=False)
    for r, (metric, title) in enumerate(METRICS):
        top = max(float(rows[k][metric]) for by_v in series.values() for rows in by_v.values()
                  for k, *_ in scenarios if k in rows)
        for c, (key, _, label) in enumerate(scenarios):
            ax = axes[r][c]
            ax.set_facecolor(SURFACE)
            for profile, by_version in series.items():
                points = [(versions.index(v), float(rows[key][metric])) for v, rows in by_version.items() if key in rows]
                if not points:
                    continue
                style = PROFILE_STYLE[profile]
                xs, ys = zip(*points)
                ax.plot(xs, ys, marker="o", markersize=5.5, linewidth=1.6, color=style["color"],
                        linestyle=style["linestyle"], markerfacecolor=style["markerfacecolor"],
                        markeredgecolor=style["color"], label=style["label"], zorder=3)
                for x, y in points:
                    # Where two series share a version (2.6.0), the higher point's label goes above, the lower below.
                    others = [float(rows[key][metric]) for pr, by_v in series.items() if pr != profile
                              for v, rows in by_v.items() if versions.index(v) == x and key in rows]
                    above = all(y >= o for o in others)
                    ax.annotate(f"{y:.0f}", (x, y), textcoords="offset points", xytext=(0, 7 if above else -13),
                                ha="center", fontsize=8, color=INK if profile == "scratch" else INK_2)
            ax.set_xlim(-0.5, len(versions) - 0.5)
            ax.set_ylim(0, top * 1.15)
            ax.set_xticks(range(len(versions)), versions)
            if r == 0:
                ax.set_title(label.replace("\n", " "), loc="left", fontsize=11, color=INK, fontweight="bold", pad=10)
            if c == 0:
                ax.set_ylabel(f"{title} (MB)")
            ax.yaxis.grid(True, color=GRID, linewidth=0.8)
            ax.set_axisbelow(True)
            for side in ("top", "right", "left"):
                ax.spines[side].set_visible(False)
            ax.spines["bottom"].set_color(GRID)
            ax.tick_params(length=0)
    handles, names = axes[0][0].get_legend_handles_labels()
    for profile, style in PROFILE_STYLE.items():
        if style["label"] not in names and profile in series:
            handles.append(plt.Line2D([], [], color=style["color"], linestyle=style["linestyle"], marker="o",
                                      markerfacecolor=style["markerfacecolor"]))
            names.append(style["label"])
    fig.legend(handles, names, loc="upper right", ncol=2, frameon=False, fontsize=9.5, bbox_to_anchor=(0.99, 0.955))
    os_title = {"windows": "Windows", "linux": "Linux"}.get(args.os, args.os)
    fig.suptitle(f"Luminous memory by version ({os_title}, release build)", x=0.01, ha="left", fontsize=13,
                 fontweight="bold", color=INK, y=0.985)
    note = ("Total across the main process and its WebView child processes. Library size and window changed over the "
            "history, and the two profile kinds are not comparable with each other: read a trend within one line.")
    if args.runs > 1:
        note += f" Mean of the newest {args.runs} runs per version (fewer where fewer exist)."
    fig.text(0.01, 0.012, textwrap.fill(note, 200), fontsize=8.5, color=INK_2, va="bottom")
    fig.tight_layout(rect=(0, 0.05, 1, 0.93), h_pad=3)
    fig.savefig(out, dpi=150, facecolor=SURFACE)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--baseline")
    p.add_argument("--candidate")
    p.add_argument("--trend", action="store_true", help="chart every recorded version instead of comparing two")
    p.add_argument("--os", default="windows")
    p.add_argument("--csv", default=REPO_ROOT / "docs" / "performance-history.csv", type=Path)
    p.add_argument("--out", type=Path, help="default: docs/performance-<baseline>-vs-<candidate>.png")
    p.add_argument("--runs", type=int, default=1, help="average the newest N runs per version (default 1)")
    p.add_argument("--profile", default="scratch", choices=["scratch", "real"],
                   help="compare rows from throwaway profiles (default) or the real profile (history before #1197)")
    args = p.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")  # the delta table uses Δ/−, which cp1252 consoles can't encode

    rows = list(csv.DictReader(args.csv.open(encoding="utf8")))
    if args.trend:
        out = args.out or REPO_ROOT / "docs" / "performance-trend.png"
        trend_chart(trend_series(rows, args.os, args.runs), args, out)
        print(f"Wrote {out}")
        return
    if not (args.baseline and args.candidate):
        p.error("--baseline and --candidate are required unless --trend is given")
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
