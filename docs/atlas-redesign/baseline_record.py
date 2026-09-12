#!/usr/bin/env python3
import argparse
import hashlib
import json
import re
import statistics
import sys


SECRET_PATTERNS = [
    re.compile(r"sk-[A-Za-z0-9_\-]+"),
    re.compile(r"ghp_[A-Za-z0-9]+"),
    re.compile(r"xox[bpas]-[A-Za-z0-9\-]+"),
    re.compile(r"AKIA[0-9A-Z]{16}"),
    re.compile(r"Bearer\s+\S+"),
    re.compile(r"(?i)(api[_-]?key|token|secret|password)\s*[:=]\s*\S+"),
]


def redact_value(text):
    redacted = text
    for pattern in SECRET_PATTERNS:
        redacted = pattern.sub("[redacted]", redacted)
    return redacted


def redact_env(env):
    return {
        key: redact_value(value) if isinstance(value, str) else value
        for key, value in env.items()
    }


def sha256_text(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def median_of(values):
    return statistics.median(values)


def spread_of(values):
    return max(values) - min(values) if values else 0.0


def summarize(values):
    return (median_of(values), spread_of(values))


def parse_trials(raw):
    return [float(v) for v in raw.split(",") if v]


def render_record(env, trials, counts, old, enrichment=None, tasks=None, graft=None, meta=None):
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
    record = {"env": redact_env(env), "structural": structural, "old_single_local_runs_s": old}
    if enrichment is not None:
        record["enrichment"] = enrichment
    if tasks is not None:
        record["tasks"] = tasks
    if graft is not None:
        record["graft"] = graft
    if meta is not None:
        record["meta"] = meta
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
    if "graft" in record:
        graft = record["graft"]
        lines.append("graft rev: " + graft["rev"] + " (trailhq/Graft HEAD)")
        lines.append(
            "graft separation: " + ("true" if graft["separation"] else "false")
        )
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


def verify_graft(rev, build_src, cli_src, grammars):
    separation = "enrichGraph" in build_src and "writeExtractCache" in build_src
    if "checkpoint" in build_src and "writeGraph" in build_src:
        checkpointing = (
            "extract cache written before enrichment; meaning pass checkpoints "
            "via writeGraph and folds prior meaning back by body_hash"
        )
    else:
        checkpointing = "no checkpoint markers found in pinned build.ts"
    if "discoverScopes" in build_src and "scopeOf" in build_src:
        scope = (
            "discoverScopes plus scopeOf; sub-scopes below MIN_SCOPE_NODES merge "
            "into root; onlyDirs filter recorded in fingerprint"
        )
    else:
        scope = "no scope markers found in pinned build.ts"
    if "walkDir" in build_src:
        discovery = "walkDir over readIncludeDirs roots with onlyDirs filter"
    else:
        discovery = "no walk markers found in pinned build.ts"
    if "discoverWorkspaceChildren" in cli_src:
        discovery += "; cli workspace parent federates queries across git children"
    unsupported = [
        "no speed advantage over atlas claimed",
        "no correctness advantage claimed",
        "workload and mechanisms only",
    ]
    limits = []
    if "TS/Python" in build_src:
        limits.append("build header scopes the M1 walk to TS/Python source files")
    if grammars:
        limits.append(
            "tree-sitter grammars shipped at pinned rev: " + ", ".join(sorted(grammars))
        )
    else:
        limits.append("language coverage not determined from pinned files")
    if "errors.push" in build_src:
        limits.append("per-file parse failures recorded; unknown files stay empty")
    return {
        "rev": rev,
        "source": "https://github.com/trailhq/Graft",
        "files": [
            {"path": "src/graph/build.ts", "sha256": sha256_text(build_src)},
            {"path": "src/cli.ts", "sha256": sha256_text(cli_src)},
        ],
        "separation": separation,
        "checkpointing": checkpointing,
        "scope": scope,
        "discovery": discovery,
        "unsupported": unsupported,
        "limits": limits,
    }


def render_markdown(record):
    env = record["env"]
    structural = record["structural"]
    old = record["old_single_local_runs_s"]
    meta = record.get("meta", {})
    lines = [
        "# atlas baseline " + meta.get("date", ""),
        "",
        "## env",
        "",
        "- mcptools rev: " + env["mcptools_rev"] + env["mcptools_dirty"],
        "- llm_stream rev: " + env["llm_stream_rev"] + env["llm_stream_dirty"],
        "- binary: " + env["binary_version"],
        "- repo files/dirs: "
        + str(env["file_count"])
        + "/"
        + str(env["dir_count"]),
        "- index files/dirs/symbols: "
        + str(structural["files"])
        + "/"
        + str(structural["dirs"])
        + "/"
        + str(structural["symbols"]),
        "- hardware: " + env["hardware"],
        "- cache: " + env["cache_state"],
    ]
    if meta.get("run_dir"):
        lines.append("- raw run dir: " + meta["run_dir"] + " (trial DBs under trial-N/index.db)")
    lines += ["", "## structural", ""]
    for name in ("initial", "incremental", "tree", "peek"):
        entry = structural[name + "_s"]
        lines.append(
            "- "
            + name
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
    if "enrichment" in record:
        lines += ["", "## enrichment", "", "- " + render_enrichment_line(record["enrichment"])]
        for task in record.get("tasks", []):
            lines.append("- " + render_task_line(task))
    if "graft" in record:
        graft = record["graft"]
        lines += [
            "",
            "## graft",
            "",
            "- rev: " + graft["rev"],
            "- source: " + graft["source"],
        ]
        for item in graft["files"]:
            lines.append("- " + item["path"] + " sha256: " + item["sha256"])
        lines += [
            "- separation of structural parse and enrichment: "
            + ("true" if graft["separation"] else "false"),
            "- checkpointing: " + graft["checkpointing"],
            "- scope: " + graft["scope"],
            "- discovery: " + graft["discovery"],
        ]
        for claim in graft["unsupported"]:
            lines.append("- unsupported claim: " + claim)
        for limit in graft["limits"]:
            lines.append("- language limit: " + limit)
    lines += [
        "",
        "Old figures are single local runs shown for context only.",
    ]
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
    parser.add_argument("--graft-rev", default="")
    parser.add_argument("--graft-build", default="")
    parser.add_argument("--graft-cli", default="")
    parser.add_argument("--graft-queries", default="")
    parser.add_argument("--markdown", action="store_true")
    parser.add_argument("--artifact-date", default="")
    parser.add_argument("--run-dir", default="")
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
    graft = None
    meta = None
    if args.artifact_date or args.run_dir:
        meta = {"date": args.artifact_date, "run_dir": args.run_dir}
    if args.graft_rev:
        with open(args.graft_build) as handle:
            build_src = handle.read()
        with open(args.graft_cli) as handle:
            cli_src = handle.read()
        grammars = [g for g in args.graft_queries.split(",") if g]
        graft = verify_graft(args.graft_rev, build_src, cli_src, grammars)
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
    record = render_record(env, trials, counts, old, enrichment, tasks, graft, meta)
    if args.markdown:
        sys.stderr.write(render_console(record))
        sys.stdout.write(render_markdown(record))
    elif args.json:
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
