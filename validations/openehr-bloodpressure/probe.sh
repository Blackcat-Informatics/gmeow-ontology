#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# Validator-zoo probe for the openEHR blood-pressure in-band-complement claim.
#
# Drives a running EHRbase CDR: uploads the Blutdruck OPT, then POSTs BOTH the vendored
# source composition and the GMEOW-augmented composition, and reports a single falsifiable
# line — PASS if both are accepted, or BOUNDARY naming the exact field/HTTP status the CDR
# rejects. This is the empirical half of usecase_openehr_bloodpressure.md §5/§9; the
# structural + data round-trip is already proven in-gate by the Rust test suite.
#
# Honest caveat (carried from the use-case doc §5): the complement rides in
# feeder_audit.original_content, whose RM meaning is "lineage from a *feeder* system".
# GMEOW is the canonical source, not a feeder — so a PASS here confirms only that the OPT
# does not constrain that RM-level slot (validation-transparency). It does NOT bless the
# semantic propriety of the carrier. The clean alternatives are a dedicated RM extension or
# a content-hash-bound sidecar (doc §5).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES="$HERE/../../docs/APPLIED_CATEGORY_THEORY/fixtures"
OPT="$HERE/Blutdruck.opt"
RESULT="$HERE/result.txt"

# Per-run temp files for the CDR HTTP response bodies — mktemp (never a predictable
# /tmp path, which is unsafe on a multi-user host) with a trap to clean up on exit.
OPT_RESP="$(mktemp)"
COMP_RESP="$(mktemp)"
trap 'rm -f "$OPT_RESP" "$COMP_RESP"' EXIT

BASE="${EHRBASE_URL:-http://localhost:8080/ehrbase}"
ADMIN_USER="${EHRBASE_ADMIN_USER:-ehrbase-admin}"
ADMIN_PASS="${EHRBASE_ADMIN_PASS:-EvenMoreSecretPassword}"
AUTH=(-u "$ADMIN_USER:$ADMIN_PASS")
API="$BASE/rest/openehr/v1"

fail_boundary() {
  echo "BOUNDARY $*" | tee "$RESULT"
  exit 1
}

# 1. Wait for EHRbase to be reachable.
echo "waiting for EHRbase at $BASE ..."
for i in $(seq 1 60); do
  if curl -fsS "${AUTH[@]}" "$BASE/management/health" >/dev/null 2>&1 \
     || curl -fsS "${AUTH[@]}" "$API/definition/template/adl1.4" >/dev/null 2>&1; then
    echo "EHRbase is up."
    break
  fi
  [ "$i" = 60 ] && fail_boundary "ehrbase-unreachable: CDR never became healthy at $BASE"
  sleep 5
done

# 2. Upload the Blutdruck OPT (idempotent — SYSTEM_ALLOWTEMPLATEOVERWRITE=true).
echo "uploading Blutdruck.opt ..."
opt_code=$(curl -s -o "$OPT_RESP" -w "%{http_code}" "${AUTH[@]}" \
  -H "Content-Type: application/xml" -H "Accept: application/xml" \
  -X POST "$API/definition/template/adl1.4" --data-binary "@$OPT")
case "$opt_code" in
  200|201|204) echo "OPT accepted ($opt_code)." ;;
  409) echo "OPT already present ($opt_code) — continuing." ;;
  *) fail_boundary "opt-upload: Blutdruck.opt rejected (HTTP $opt_code): $(cat "$OPT_RESP")" ;;
esac

# 3. Create an EHR to hold the compositions. The ehr_id is the last path segment of the
#    Location header (robust against JSON whitespace in the body).
ehr_location=$(curl -s -D - -o /dev/null "${AUTH[@]}" -H "Accept: application/json" \
  -X POST "$API/ehr" | tr -d '\r' | sed -n 's#^[Ll]ocation:[[:space:]]*##p' | head -1)
ehr_id="${ehr_location##*/}"
[ -n "$ehr_id" ] || fail_boundary "ehr-create: could not create an EHR (no Location header)"
echo "EHR created: $ehr_id"

# 4. POST a composition; expect 201 Created. Names the boundary on any other status.
post_composition() {
  local label="$1" file="$2"
  local code
  code=$(curl -s -o "$COMP_RESP" -w "%{http_code}" "${AUTH[@]}" \
    -H "Content-Type: application/json" -H "Accept: application/json" \
    -H "Prefer: return=representation" \
    -X POST "$API/ehr/$ehr_id/composition" --data-binary "@$file")
  if [ "$code" = "201" ] || [ "$code" = "200" ]; then
    echo "  $label: accepted ($code)"
  else
    fail_boundary "$label: composition rejected (HTTP $code): $(head -c 800 "$COMP_RESP")"
  fi
}

echo "validating compositions under Blutdruck.opt ..."
post_composition "source"    "$FIXTURES/blood_pressure.source.json"
post_composition "augmented" "$FIXTURES/blood_pressure.augmented.json"

echo "PASS source+augmented validate under Blutdruck.opt" | tee "$RESULT"
