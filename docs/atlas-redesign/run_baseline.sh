#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RENDER="$SCRIPT_DIR/baseline_record.py"

TRIALS=3
REPO="$HOME/Projects/Rust/llm_stream"
PEEK_TARGET="crates/llm_stream/src/claude.rs"
JSON_OUT=0
PROVIDER=""
MODEL=""
ENRICH_PARALLEL=4

usage() {
  echo "usage: run_baseline.sh [--trials N] [--repo PATH] [--peek PATH] [--json] [--provider NAME --model NAME]"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --trials) TRIALS="$2"; shift 2 ;;
    --repo) REPO="$2"; shift 2 ;;
    --peek) PEEK_TARGET="$2"; shift 2 ;;
    --json) JSON_OUT=1; shift ;;
    --provider) PROVIDER="$2"; shift 2 ;;
    --model) MODEL="$2"; shift 2 ;;
    --help) usage; exit 0 ;;
    *) echo "unknown flag: $1" >&2; usage >&2; exit 1 ;;
  esac
done

if [ -n "$PROVIDER" ] && [ -z "$MODEL" ]; then
  echo "--provider needs --model" >&2; usage >&2; exit 1
fi
if [ -z "$PROVIDER" ] && [ -n "$MODEL" ]; then
  echo "--model needs --provider" >&2; usage >&2; exit 1
fi

if [ ! -d "$REPO/.git" ]; then
  echo "repo has no .git: $REPO" >&2
  exit 1
fi
if [ ! -f "$REPO/$PEEK_TARGET" ]; then
  echo "peek target missing: $REPO/$PEEK_TARGET" >&2
  exit 1
fi

cargo build -q -p mcptools --manifest-path "$ROOT/Cargo.toml" >&2
BIN="$ROOT/target/debug/mcptools"

BASE="${TMPDIR%/}/opencode"
RUN_DIR="$BASE/atlas-baseline-$$"
mkdir -p "$RUN_DIR"

dirty_tag() {
  if [ -n "$(git -C "$1" status --porcelain)" ]; then
    echo " (dirty)"
  else
    echo ""
  fi
}

MC_REV="$(git -C "$ROOT" rev-parse HEAD)"
MC_DIRTY="$(dirty_tag "$ROOT")"
LS_REV="$(git -C "$REPO" rev-parse HEAD)"
LS_DIRTY="$(dirty_tag "$REPO")"
BIN_VER="$("$BIN" --version)"
FILE_COUNT="$(git -C "$REPO" ls-files | wc -l | tr -d ' ')"
DIR_COUNT="$(find "$REPO" -type d -not -path "$REPO/.git*" -not -path "$REPO/target*" | wc -l | tr -d ' ')"
HARDWARE="$(uname -m) $(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -p)"
if [ -f "$REPO/.mcptools/atlas/primer.md" ]; then PRIMER="primer present"; else PRIMER="no primer"; fi
if [ -f "$REPO/.mcptools/atlas/index.db" ]; then LOCALDB="local db present"; else LOCALDB="no local db"; fi
CACHE_STATE="$PRIMER; $LOCALDB; fresh temp db per trial"

measure() {
  local out
  out="$({ TIMEFORMAT='%R'; time "$@" >"$RUN_DIR/last.out" 2>"$RUN_DIR/last.err"; } 2>&1)"
  local rc=$?
  printf '%s' "$out" | tail -n 1 | tr -d ' '
  return $rc
}

INITIAL_TRIALS=""
INCREMENTAL_TRIALS=""
TREE_TRIALS=""
PEEK_TRIALS=""

t=1
while [ "$t" -le "$TRIALS" ]; do
  TRIAL_DIR="$RUN_DIR/trial-$t"
  mkdir -p "$TRIAL_DIR"
  export ATLAS_DB_PATH="$TRIAL_DIR/index.db"
  cd "$REPO" || exit 1
  v="$(measure "$BIN" atlas index)" || { echo "index failed (trial $t)" >&2; cat "$RUN_DIR/last.err" >&2; exit 1; }
  if ! grep -q "structural-only" "$RUN_DIR/last.err"; then
    echo "enrichment ran during trial $t; expected structural-only" >&2
    cat "$RUN_DIR/last.err" >&2
    exit 1
  fi
  INITIAL_TRIALS="$INITIAL_TRIALS,$v"
  v="$(measure "$BIN" atlas index --incremental)" || { echo "incremental index failed (trial $t)" >&2; exit 1; }
  INCREMENTAL_TRIALS="$INCREMENTAL_TRIALS,$v"
  v="$(measure "$BIN" atlas tree)" || { echo "tree failed (trial $t)" >&2; exit 1; }
  TREE_TRIALS="$TREE_TRIALS,$v"
  v="$(measure "$BIN" atlas peek "$PEEK_TARGET")" || { echo "peek failed (trial $t)" >&2; exit 1; }
  PEEK_TRIALS="$PEEK_TRIALS,$v"
  t=$((t + 1))
done

INITIAL_TRIALS="${INITIAL_TRIALS#,}"
INCREMENTAL_TRIALS="${INCREMENTAL_TRIALS#,}"
TREE_TRIALS="${TREE_TRIALS#,}"
PEEK_TRIALS="${PEEK_TRIALS#,}"
export ATLAS_DB_PATH="$RUN_DIR/trial-$TRIALS/index.db"

STATUS_OUT="$(cd "$REPO" && "$BIN" atlas status)"
FILES="$(printf '%s' "$STATUS_OUT" | sed -n 's/^Files: *\([0-9]*\).*/\1/p')"
DIRS="$(printf '%s' "$STATUS_OUT" | sed -n 's/^Directories: *\([0-9]*\).*/\1/p')"
SYMBOLS="$(printf '%s' "$STATUS_OUT" | sed -n 's/^Symbols: *\([0-9]*\).*/\1/p')"

ARGS=(--mc-rev "$MC_REV" --mc-dirty "$MC_DIRTY" --ls-rev "$LS_REV" --ls-dirty "$LS_DIRTY"
  --binary "$BIN_VER" --file-count "$FILE_COUNT" --dir-count "$DIR_COUNT"
  --hardware "$HARDWARE" --cache "$CACHE_STATE"
  --initial "$INITIAL_TRIALS" --incremental "$INCREMENTAL_TRIALS"
  --tree "$TREE_TRIALS" --peek "$PEEK_TRIALS"
  --files "$FILES" --dirs "$DIRS" --symbols "$SYMBOLS")
if [ "$JSON_OUT" -eq 1 ]; then
  ARGS+=(--json)
fi
if [ -z "$PROVIDER" ]; then
  python3 "$RENDER" "${ARGS[@]}"
  echo "temp db: $RUN_DIR" >&2
  echo "enrichment stage: not built yet" >&2
  exit 0
fi

TASK_DB="$RUN_DIR/trial-$TRIALS/index.db"
ENRICH_STATUS=""
ENRICH_REASON=""
ENRICH_REQUESTS=0
ENRICH_ELAPSED=0
ENRICH_PRIMER_SHA=""

if [ "$PROVIDER" != "ollama" ]; then
  ENRICH_STATUS="missing"
  ENRICH_REASON="provider '$PROVIDER' unsupported (atlas enriches through ollama only)"
else
  ENRICH_DIR="$RUN_DIR/enrich"
  mkdir -p "$ENRICH_DIR"
  cat > "$ENRICH_DIR/primer.md" <<'PRIMER_EOF'
# Project Primer

## What is this project?
llm_stream is a Rust CLI that streams prompts to many LLM providers from the terminal.

## How is it built?
Cargo workspace. Provider adapters live under crates/llm_stream/src. Async runtime on tokio.
PRIMER_EOF
  ENRICH_PRIMER_SHA="$(shasum -a 256 "$ENRICH_DIR/primer.md" | cut -d' ' -f1)"
  export ATLAS_DB_PATH="$ENRICH_DIR/index.db"
  export ATLAS_PRIMER_PATH="$ENRICH_DIR/primer.md"
  export ATLAS_FILE_MODEL="$MODEL"
  export ATLAS_DIR_MODEL="$MODEL"
  cd "$REPO" || exit 1
  ENRICH_ELAPSED="$(measure "$BIN" atlas index --parallel "$ENRICH_PARALLEL")" || {
    ENRICH_STATUS="missing"
    ENRICH_REASON="index failed: $(grep -v '^$' "$RUN_DIR/last.err" | tail -n 1 | cut -c1-160)"
  }
  if [ -z "$ENRICH_STATUS" ]; then
    COUNTS="$(python3 "$RENDER" --count-db "$ENRICH_DIR/index.db")"
    DESCRIBED="$(printf '%s' "$COUNTS" | python3 -c 'import json,sys; c=json.load(sys.stdin); print(c["files_described"]+c["dirs_described"])')"
    FAILED="$(printf '%s' "$COUNTS" | python3 -c 'import json,sys; c=json.load(sys.stdin); print(c["files_failed"]+c["dirs_failed"])')"
    if [ "$DESCRIBED" -eq 0 ]; then
      ENRICH_STATUS="missing"
      ENRICH_REASON="enrichment produced no descriptions"
    else
      ENRICH_STATUS="ok"
      ENRICH_REQUESTS=$((DESCRIBED + FAILED))
    fi
  fi
  unset ATLAS_PRIMER_PATH ATLAS_FILE_MODEL ATLAS_DIR_MODEL
  export ATLAS_DB_PATH="$TASK_DB"
fi

score_task() {
  local name="$1"
  local file="$2"
  local symbol="$3"
  local trials=""
  local correct=1
  local t=1
  while [ "$t" -le "$TRIALS" ]; do
    v="$(measure "$BIN" atlas peek "$file")" || { echo "peek failed for task $name (trial $t)" >&2; exit 1; }
    trials="$trials,$v"
    if ! grep -qF "$symbol" "$RUN_DIR/last.out"; then
      correct=0
    fi
    t=$((t + 1))
  done
  printf '%s|%s|%s|%s|%s' "$name" "$file" "$symbol" "${trials#,}" "$correct"
}

cd "$REPO" || exit 1
ARGS+=(--provider "$PROVIDER" --model "$MODEL")
if [ "$ENRICH_STATUS" = "ok" ]; then
  ARGS+=(--enrich-status ok --enrich-requests "$ENRICH_REQUESTS" --enrich-elapsed "$ENRICH_ELAPSED"
    --enrich-primer-sha "$ENRICH_PRIMER_SHA" --enrich-parallel "$ENRICH_PARALLEL")
else
  ARGS+=(--enrich-status missing --enrich-reason "$ENRICH_REASON")
fi
ARGS+=(--task "$(score_task locate-claude-runner crates/llm_stream/src/claude.rs 'pub async fn run')")
ARGS+=(--task "$(score_task locate-entry-point crates/llm_stream/src/main.rs 'async fn main')")
ARGS+=(--task "$(score_task locate-auth-login crates/llm_stream/src/auth/flow.rs 'pub fn login')")
python3 "$RENDER" "${ARGS[@]}"
echo "temp db: $RUN_DIR" >&2
echo "graft/artifact stage: not built yet" >&2
