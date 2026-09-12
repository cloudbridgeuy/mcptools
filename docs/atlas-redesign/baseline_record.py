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


def render_record(env, trials, counts, old, enrichment=None, tasks=None):
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
    record = {"env": env, "structural": structural, "old_single_local_runs_s": old}
    if enrichment is not None:
        record["enrichment"] = enrichment
    if tasks is not None:
        record["tasks"] = tasks
    return record


def enrichment_ok(provider, model, requests, elapsed_s, primer_sha256, parallel):
    return {
        "status": "ok",
        "provider": provider,
        "model": model,
        "requests": requests,
        "elapsed_s": round(elapsed_s, 3),
        "primer_sha256": primer_sha256,
        "parallel": parallel,
    }


def enrichment_missing(provider, model, reason):
    return {
        "status": "missing",
        "provider": provider,
        "model": model,
        "reason": reason,
    }


def parse_task(raw):
    name, target_file, target_symbol, trials_raw, correct_raw = raw.split("|", 4)
    values = [float(v) for v in trials_raw.split(",") if v]
    median, spread = summarize(values)
    return {
        "task": name,
        "target_file": target_file,
        "target_symbol": target_symbol,
        "time_to_context_s": {
            "median": round(median, 3),
            "spread": round(spread, 3),
            "trials": values,
        },
        "correct": correct_raw == "1",
    }


def count_descriptions(db_path):
    import sqlite3

    failed_marker = "[description failed]"
    db = sqlite3.connect(db_path)
    try:
        files = db.execute(
            "SELECT short_description FROM files"
        ).fetchall()
        dirs = db.execute(
            "SELECT short_description FROM directories"
        ).fetchall()
    finally:
        db.close()
    described = lambda rows: sum(1 for (s,) in rows if s and s != failed_marker)
    failed = lambda rows: sum(1 for (s,) in rows if s == failed_marker)
    return {
        "files_described": described(files),
        "files_failed": failed(files),
        "dirs_described": described(dirs),
        "dirs_failed": failed(dirs),
    }


def render_console(record):
    env = record["env"]
    structural = record["structural"]
    old = record["old_single_local_runs_s"]
    gated = "enrichment" in record
    title = (
        "atlas baseline (gated enrichment, explicit provider)"
        if gated
        else "atlas baseline (structural only, no model calls)"
    )
    lines = [
        title,
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
    if not gated:
        lines.append("enrichment: not built yet")
        return "\n".join(lines) + "\n"
    lines.append(render_enrichment_line(record["enrichment"]))
    for task in record.get("tasks", []):
        lines.append(render_task_line(task))
    return "\n".join(lines) + "\n"


def render_enrichment_line(enrichment):
    if enrichment["status"] == "ok":
        return (
            "enrichment: ok (provider "
            + enrichment["provider"]
            + " model "
            + enrichment["model"]
            + ", "
            + str(enrichment["requests"])
            + " requests, "
            + format(enrichment["elapsed_s"], ".3f")
            + "s elapsed)"
        )
    return "enrichment: missing (" + enrichment["reason"] + ")"


def render_task_line(task):
    timing = task["time_to_context_s"]
    return (
        "task "
        + task["task"]
        + ": "
        + format(timing["median"], ".3f")
        + "s median spread "
        + format(timing["spread"], ".3f")
        + "s over "
        + str(len(timing["trials"]))
        + " trials target "
        + task["target_file"]
        + " :: "
        + task["target_symbol"]
        + " correct="
        + ("true" if task["correct"] else "false")
    )


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
    parser.add_argument("--provider", default="")
    parser.add_argument("--model", default="")
    parser.add_argument("--enrich-status", default="")
    parser.add_argument("--enrich-reason", default="")
    parser.add_argument("--enrich-requests", type=int, default=0)
    parser.add_argument("--enrich-elapsed", type=float, default=0.0)
    parser.add_argument("--enrich-primer-sha", default="")
    parser.add_argument("--enrich-parallel", type=int, default=0)
    parser.add_argument("--task", action="append", default=[])
    parser.add_argument("--count-db", default="")
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
    enrichment = None
    tasks = None
    if args.enrich_status:
        if args.enrich_status == "ok":
            enrichment = enrichment_ok(
                args.provider,
                args.model,
                args.enrich_requests,
                args.enrich_elapsed,
                args.enrich_primer_sha,
                args.enrich_parallel,
            )
        else:
            enrichment = enrichment_missing(
                args.provider, args.model, args.enrich_reason
            )
        tasks = [parse_task(raw) for raw in args.task]
    record = render_record(env, trials, counts, old, enrichment, tasks)
    if args.json:
        sys.stderr.write(render_console(record))
        sys.stdout.write(json.dumps(record, indent=2) + "\n")
    else:
        sys.stdout.write(render_console(record))
    return 0


if __name__ == "__main__":
    if "--count-db" in sys.argv[1:]:
        target = sys.argv[sys.argv.index("--count-db") + 1]
        sys.stdout.write(json.dumps(count_descriptions(target)) + "\n")
    else:
        sys.exit(main(sys.argv[1:]))
