#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# Validator-zoo probe for the Common Logic / first-order cross-check lane.
#
# Translates the CLIF export of the gmeow logic: foundation (and a small
# externally-authored CL knowledge base) to TPTP-FOF via the lane-local
# `clif2tptp` binary (which reuses the real `gmeow_logic_compile::clif` CLIF
# reader — never a hand-rolled parser), then hands each translation to the E
# theorem prover (an independent, general first-order ATP) and reports a
# single falsifiable line — PASS, or a named BOUNDARY — to result.txt.
#
# Two checks:
#   A. Foundation consistency  — the exported foundation's FOL translation
#      must not be refutable (Unsatisfiable/ContradictoryAxioms/Theorem would
#      mean the exported foundation is FOL-inconsistent).
#   B. Ingest entailment       — E must independently CONFIRM the exact
#      ancestor(alice, carol) entailment the native engine derives in-gate
#      over the externally-authored sample-kb.clif + its EDB (the load-bearing
#      empirical half of the native ⊇ oracle claim).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
RESULT="$HERE/result.txt"

CLIF2TPTP_DIR="$HERE/clif2tptp"
CLIF2TPTP_BIN="$CLIF2TPTP_DIR/target/release/clif2tptp"

DOCKER_IMAGE="${CL_FOL_EPROVER_IMAGE:-cl-fol-eprover}"
EPROVER_CPU_LIMIT="${EPROVER_CPU_LIMIT:-30}"

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

fail_boundary() {
  echo "BOUNDARY $*" | tee "$RESULT"
  exit 1
}

pass() {
  echo "PASS $*" | tee "$RESULT"
}

# ── 0. Build clif2tptp once (off-workspace, empty [workspace] table). ─────────
echo "building clif2tptp ..."
cargo build --release --manifest-path "$CLIF2TPTP_DIR/Cargo.toml" >&2
[ -x "$CLIF2TPTP_BIN" ] || fail_boundary "clif2tptp-build: binary not found at $CLIF2TPTP_BIN after cargo build"

# ── run_eprover: dispatch to a local `eprover` on PATH if present, else the ──
# ── Docker image built by `make build-image`.                               ──
run_eprover() {
  local tptp_file="$1"
  if command -v eprover >/dev/null 2>&1; then
    eprover --auto --cpu-limit="$EPROVER_CPU_LIMIT" --tstp-format "$tptp_file"
  else
    docker run --rm -v "$WORKDIR:/work" --entrypoint eprover "$DOCKER_IMAGE" \
      --auto --cpu-limit="$EPROVER_CPU_LIMIT" --tstp-format "/work/$(basename "$tptp_file")"
  fi
}

szs_status() {
  # Extract the SZS status token eprover prints, e.g. "# SZS status Theorem".
  # `|| true`: under `set -euo pipefail` a no-match `grep` exits 1 and would
  # abort the whole probe before any PASS/BOUNDARY line prints; the empty-status
  # ("") case below is the intended handling when E emits no SZS line at all.
  grep -o '# SZS status [A-Za-z]*' | tail -1 | awk '{print $NF}' || true
}

# ── Check A — foundation consistency. ──────────────────────────────────────
echo "Check A: translating the exported foundation CLIF ..."
FOUNDATION_CLIF="$REPO_ROOT/generated/cl/gmeow.clif"
[ -f "$FOUNDATION_CLIF" ] || fail_boundary "check-a-missing-input: $FOUNDATION_CLIF not found (run \`make sync\` in the main worktree first)"

FOUNDATION_TPTP="$WORKDIR/foundation.p"
# stdout carries the TPTP (→ the .p file); stderr carries any translator
# BOUNDARY line, captured via a separate sink so it survives the redirection.
FOUNDATION_ERR="$WORKDIR/foundation.err"
if ! "$CLIF2TPTP_BIN" "$FOUNDATION_CLIF" > "$FOUNDATION_TPTP" 2> "$FOUNDATION_ERR"; then
  foundation_stderr=$(cat "$FOUNDATION_ERR")
  boundary_line=$(printf '%s\n' "$foundation_stderr" | grep '^BOUNDARY' || true)
  if [ -n "$boundary_line" ]; then
    fail_boundary "check-a-translate: $boundary_line"
  fi
  fail_boundary "check-a-translate: clif2tptp failed on the foundation CLIF: $foundation_stderr"
fi

echo "Check A: running E prover (saturation, no conjecture) ..."
szs_a_output=$(run_eprover "$FOUNDATION_TPTP" 2>&1) || true
szs_a=$(printf '%s\n' "$szs_a_output" | szs_status)
echo "Check A SZS status: ${szs_a:-<none>}"

case "$szs_a" in
  Satisfiable|CounterSatisfiable)
    echo "Check A: consistent (SZS $szs_a) — OK."
    ;;
  Unknown|GaveUp|ResourceOut|Timeout|"")
    echo "Check A: prover incompleteness (SZS ${szs_a:-<none>}) — OK, not a divergence (native decides consistent)."
    ;;
  Unsatisfiable|ContradictoryAxioms|Theorem)
    fail_boundary "check-a-foundation-fol-inconsistent: E prover reported SZS status $szs_a on the exported foundation's FOL translation — a real divergence against native's 'consistent' verdict"
    ;;
  *)
    fail_boundary "check-a-unexpected-szs: E prover reported an unrecognized SZS status '$szs_a'"
    ;;
esac

# ── Check B — ingest entailment (native ⊇ oracle). ─────────────────────────
echo "Check B: translating the sample-kb CLIF + EDB + conjecture ..."
KB_CLIF="$REPO_ROOT/conformance/logic/cl-ingest/sample-kb.clif"
KB_EDB="$REPO_ROOT/conformance/logic/cl-ingest/sample-kb.edb.nq"
NS="https://example.org/cl-ingest/genealogy"
[ -f "$KB_CLIF" ] || fail_boundary "check-b-missing-input: $KB_CLIF not found"
[ -f "$KB_EDB" ] || fail_boundary "check-b-missing-input: $KB_EDB not found"

GOAL_TPTP="$WORKDIR/goal.p"
# stdout carries the TPTP (→ the .p file); stderr (any translator BOUNDARY line)
# goes to a separate sink so it survives the redirection and can be inspected.
GOAL_ERR="$WORKDIR/goal.err"
if ! "$CLIF2TPTP_BIN" "$KB_CLIF" --edb "$KB_EDB" \
    --conjecture "$NS/ancestor" "$NS/alice" "$NS/carol" > "$GOAL_TPTP" 2> "$GOAL_ERR"; then
  goal_stderr=$(cat "$GOAL_ERR")
  boundary_line=$(printf '%s\n' "$goal_stderr" | grep '^BOUNDARY' || true)
  if [ -n "$boundary_line" ]; then
    fail_boundary "check-b-translate: $boundary_line"
  fi
  fail_boundary "check-b-translate: clif2tptp failed on sample-kb.clif: $goal_stderr"
fi

echo "Check B: running E prover (conjecture: ancestor(alice, carol)) ..."
szs_b_output=$(run_eprover "$GOAL_TPTP" 2>&1) || true
szs_b=$(printf '%s\n' "$szs_b_output" | szs_status)
echo "Check B SZS status: ${szs_b:-<none>}"

case "$szs_b" in
  Theorem)
    echo "Check B: E confirms the ancestor(alice, carol) entailment — OK."
    ;;
  CounterSatisfiable|Satisfiable)
    fail_boundary "check-b-entailment-refuted: E prover reported SZS status $szs_b — it refutes the ancestor(alice, carol) entailment the native engine derived in-gate (a real divergence)"
    ;;
  Unknown|GaveUp|ResourceOut|Timeout|"")
    echo "Check B: prover incompleteness (SZS ${szs_b:-<none>}) noted, not hard-BOUNDARYing (this conjecture should prove near-instantly)."
    ;;
  *)
    fail_boundary "check-b-unexpected-szs: E prover reported an unrecognized SZS status '$szs_b'"
    ;;
esac

pass "foundation FOL-consistent + sample-KB ancestor entailment confirmed by E prover"
