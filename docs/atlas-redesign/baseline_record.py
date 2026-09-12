#!/usr/bin/env python3
import argparse
import json
import statistics
import sys


def median_of(values):
    return statistics.median(values)


def spread_of(values):
    return max(values) - min(values) if values else 0.0


def summarize(values):
    return (median_of(values), spread_of(values))


def parse_trials(raw):
    return [float(v) for v in raw.split(",") if v]


def render_record(env, trials, counts, old):
    structural = {}
    for name in ("initial", "incremental", "tree", "peek"):
        median, spread = summarize(trials[name])
        structural[name + "_s"] = {
            "median": round(median, 3),
            "spread": round(spread, 3),
            "trials": trials[name],
        }
    structural["files"] = counts["files"]
    structural["dirs"] = counts["dirs"]
    structural["symbols"] = counts["symbols"]
    return {"env": env, "structural": structural, "old_single_local_runs_s": old}


def render_console(record):
    env = record["env"]
    structural = record["structural"]
    old = record["old_single_local_runs_s"]
    lines = [
        "atlas baseline (structural only, no model calls)",
        "mcptools rev: " + env["mcptools_rev"] + env["mcptools_dirty"],
        "llm_stream rev: " + env["llm_stream_rev"] + env["llm_stream_dirty"],
        "binary: " + env["binary_version"],
        "repo files/dirs: "
        + str(env["file_count"])
        + "/"
        + str(env["dir_count"])
        + "  index files/dirs/symbols: "
        + str(structural["files"])
        + "/"
        + str(structural["dirs"])
        + "/"
        + str(structural["symbols"]),
        "hardware: " + env["hardware"],
        "cache: " + env["cache_state"],
    ]
    for name in ("initial", "incremental", "tree", "peek"):
        entry = structural[name + "_s"]
        lines.append(
            name
            + ": "
            + format(entry["median"], ".3f")
            + "s median spread "
            + format(entry["spread"], ".3f")
            + "s over "
            + str(len(entry["trials"]))
            + " trials (old single local run "
            + format(old[name], ".3f")
            + "s)"
        )
    lines.append("enrichment: not built yet")
    return "\n".join(lines) + "\n"


def build_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mc-rev", required=True)
    parser.add_argument("--mc-dirty", default="")
    parser.add_argument("--ls-rev", required=True)
    parser.add_argument("--ls-dirty", default="")
    parser.add_argument("--binary", required=True)
    parser.add_argument("--file-count", type=int, required=True)
    parser.add_argument("--dir-count", type=int, required=True)
    parser.add_argument("--hardware", required=True)
    parser.add_argument("--cache", required=True)
    parser.add_argument("--initial", required=True)
    parser.add_argument("--incremental", required=True)
    parser.add_argument("--tree", required=True)
    parser.add_argument("--peek", required=True)
    parser.add_argument("--files", type=int, required=True)
    parser.add_argument("--dirs", type=int, required=True)
    parser.add_argument("--symbols", type=int, required=True)
    parser.add_argument("--json", action="store_true")
    return parser


def main(argv):
    args = build_parser().parse_args(argv)
    env = {
        "mcptools_rev": args.mc_rev,
        "mcptools_dirty": args.mc_dirty,
        "llm_stream_rev": args.ls_rev,
        "llm_stream_dirty": args.ls_dirty,
        "binary_version": args.binary,
        "file_count": args.file_count,
        "dir_count": args.dir_count,
        "hardware": args.hardware,
        "cache_state": args.cache,
    }
    trials = {
        "initial": parse_trials(args.initial),
        "incremental": parse_trials(args.incremental),
        "tree": parse_trials(args.tree),
        "peek": parse_trials(args.peek),
    }
    counts = {"files": args.files, "dirs": args.dirs, "symbols": args.symbols}
    old = {"initial": 0.262, "incremental": 0.051, "tree": 0.015, "peek": 0.075}
    record = render_record(env, trials, counts, old)
    if args.json:
        sys.stderr.write(render_console(record))
        sys.stdout.write(json.dumps(record, indent=2) + "\n")
    else:
        sys.stdout.write(render_console(record))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
