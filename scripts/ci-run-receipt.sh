#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Capture one completed GitHub Actions run as performance evidence. This is a
# report-only collector: it never changes a workflow run or grades correctness.

set -euo pipefail

usage() {
  cat >&2 <<EOF
usage: $0 \\
  --run-id ID --variant baseline|candidate --sample-index N \\
  --node-class CLASS --pair-id ID --cache-state cold|warm|partial \\
  --cargo-cache-state STATE --sync-cache-state STATE \\
  --pipeline-cache-state STATE --fixture-cache-state STATE \\
  --bundle-import-cache-state STATE --nextest-archive-cache-state STATE \\
  [--partial-change KIND:PATH:SHA256] --output PATH
EOF
  exit 2
}

run_id=
variant=
sample_index=
node_class=
pair_id=
cache_state=
partial_change=
output=
cargo_cache_state=
sync_cache_state=
pipeline_cache_state=
fixture_cache_state=
bundle_import_cache_state=
nextest_archive_cache_state=
while (($#)); do
  [[ $# -ge 2 ]] || usage
  case $1 in
    --run-id) run_id=$2 ;;
    --variant) variant=$2 ;;
    --sample-index) sample_index=$2 ;;
    --node-class) node_class=$2 ;;
    --pair-id) pair_id=$2 ;;
    --cache-state) cache_state=$2 ;;
    --partial-change) partial_change=$2 ;;
    --output) output=$2 ;;
    --cargo-cache-state) cargo_cache_state=$2 ;;
    --sync-cache-state) sync_cache_state=$2 ;;
    --pipeline-cache-state) pipeline_cache_state=$2 ;;
    --fixture-cache-state) fixture_cache_state=$2 ;;
    --bundle-import-cache-state) bundle_import_cache_state=$2 ;;
    --nextest-archive-cache-state) nextest_archive_cache_state=$2 ;;
    *) usage ;;
  esac
  shift 2
done
[[ "$run_id" =~ ^[1-9][0-9]*$ ]] || usage
[[ "$variant" == "baseline" || "$variant" == "candidate" ]] || usage
[[ "$sample_index" =~ ^[1-9][0-9]*$ ]] || usage
[[ -n "$node_class" ]] || usage
[[ -n "$pair_id" && -n "$output" ]] || usage
[[ "$cache_state" == "cold" || "$cache_state" == "warm" || "$cache_state" == "partial" ]] || usage
for state in \
  "$cargo_cache_state" \
  "$sync_cache_state" \
  "$pipeline_cache_state" \
  "$fixture_cache_state" \
  "$bundle_import_cache_state" \
  "$nextest_archive_cache_state"; do
  [[ "$state" == "cold" || "$state" == "warm" || "$state" == "partial" || \
    "$state" == "absent" || "$state" == "not-applicable" ]] || usage
done
if [[ "$cache_state" == "partial" ]]; then
  [[ "$partial_change" =~ ^[^:]+:.+:[0-9a-f]{64}$ ]] || usage
else
  [[ -z "$partial_change" ]] || usage
fi

for tool in gh jq sha256sum base64 date awk grep sed sort find wc; do
  command -v "$tool" >/dev/null || {
    echo "$tool is required to collect CI run evidence" >&2
    exit 1
  }
done

cache_classes=$(jq -S -n \
  --arg cargo "$cargo_cache_state" \
  --arg sync_manifest "$sync_cache_state" \
  --arg pipeline "$pipeline_cache_state" \
  --arg fixture "$fixture_cache_state" \
  --arg bundle_import "$bundle_import_cache_state" \
  --arg nextest_archive "$nextest_archive_cache_state" \
  '{
    cargo: $cargo,
    sync_manifest: $sync_manifest,
    pipeline: $pipeline,
    fixture: $fixture,
    bundle_import: $bundle_import,
    nextest_archive: $nextest_archive
  }')

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"
repository=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
tmp_dir=$(mktemp -d)
trap 'rm -rf -- "$tmp_dir"' EXIT

gh api "repos/$repository/actions/runs/$run_id" > "$tmp_dir/run.json"
gh api "repos/$repository/actions/runs/$run_id/jobs?per_page=100" > "$tmp_dir/jobs.json"
gh api "repos/$repository/actions/runs/$run_id/artifacts?per_page=100" > "$tmp_dir/artifacts.json"

[[ $(jq -r .status "$tmp_dir/run.json") == "completed" ]] || {
  echo "workflow run $run_id is not completed" >&2
  exit 1
}
[[ $(jq -r .conclusion "$tmp_dir/run.json") == "success" ]] || {
  echo "workflow run $run_id did not succeed" >&2
  exit 1
}
[[ $(jq -r .total_count "$tmp_dir/jobs.json") -le 100 ]] || {
  echo "workflow run $run_id has more than 100 jobs; pagination must be implemented" >&2
  exit 1
}
[[ $(jq -r .total_count "$tmp_dir/artifacts.json") -le 100 ]] || {
  echo "workflow run $run_id has more than 100 artifacts; pagination must be implemented" >&2
  exit 1
}

head_sha=$(jq -r .head_sha "$tmp_dir/run.json")
mapfile -t artifact_shas < <(
  jq -r '.artifacts[].name |
    select(startswith("gmeow-dev-producer-")) |
    sub("^gmeow-dev-producer-"; "")' "$tmp_dir/artifacts.json" | LC_ALL=C sort -u
)
[[ ${#artifact_shas[@]} -eq 1 && "${artifact_shas[0]}" =~ ^[0-9a-f]{40,64}$ ]] || {
  echo "workflow run $run_id must expose exactly one git-shaped producer artifact identity" >&2
  exit 1
}
# On pull_request runs `head_sha` is the branch head while `${{ github.sha }}` (and
# therefore every source-bound artifact name/receipt) is the tested merge commit. Bind
# both identities; never guess that one is the other.
artifact_sha=${artifact_shas[0]}
workflow_path=$(jq -r .path "$tmp_dir/run.json")
workflow_path=${workflow_path%%@*}
workflow_path=${workflow_path#./}
gh api "repos/$repository/contents/$workflow_path?ref=$head_sha" --jq .content |
  tr -d '\n' | base64 --decode > "$tmp_dir/workflow.yml"

artifact_exists() {
  local name=$1
  jq -e --arg name "$name" '.artifacts[] | select(.name == $name)' \
    "$tmp_dir/artifacts.json" >/dev/null
}

download_declared_artifact() {
  local name=$1
  local destination=$2
  local declaration=$3
  if artifact_exists "$name"; then
    mkdir -p "$destination"
    gh run download "$run_id" --repo "$repository" --name "$name" \
      --dir "$destination" >/dev/null
    return
  fi
  if grep -Fq -- "$declaration" "$tmp_dir/workflow.yml"; then
    echo "workflow run $run_id is missing declared evidence artifact $name" >&2
    exit 1
  fi
}

# Download only compact evidence artifacts. The authenticated nextest archive and
# generated tree are intentionally excluded: their small receipts bind those large
# payloads, so transferring gigabytes into a report-only collector would itself distort
# the measurement path.
evidence_dir=$tmp_dir/evidence
mkdir -p "$evidence_dir"
github_sha_template="\${{ github.sha }}"
matrix_generation_template="\${{ matrix.generation }}"
matrix_shard_template="\${{ matrix.shard }}"
generation_evidence_declaration="generation-evidence-${github_sha_template}-${matrix_generation_template}"
rust_prebuild_evidence_declaration="rust-prebuild-evidence-${github_sha_template}"
rust_archive_evidence_declaration="rust-archive-evidence-${github_sha_template}"
reason_evidence_declaration="reason-evidence-${github_sha_template}"
rust_shard_evidence_declaration="rust-perf-receipt-${matrix_shard_template}"
if grep -Fq -- "$generation_evidence_declaration" \
  "$tmp_dir/workflow.yml"; then
  for generation in a b; do
    download_declared_artifact \
      "generation-evidence-$artifact_sha-$generation" \
      "$evidence_dir/generation-$generation" \
      "$generation_evidence_declaration"
  done
fi
download_declared_artifact \
  "rust-prebuild-evidence-$artifact_sha" \
  "$evidence_dir/rust-prebuild" \
  "$rust_prebuild_evidence_declaration"
download_declared_artifact \
  "rust-archive-evidence-$artifact_sha" \
  "$evidence_dir/rust-archive" \
  "$rust_archive_evidence_declaration"
download_declared_artifact \
  "reason-evidence-$artifact_sha" \
  "$evidence_dir/reason" \
  "$reason_evidence_declaration"
if grep -Fq -- "$rust_shard_evidence_declaration" "$tmp_dir/workflow.yml"; then
  mapfile -t shard_artifacts < <(
    jq -r '.artifacts[].name | select(startswith("rust-perf-receipt-"))' \
      "$tmp_dir/artifacts.json" | LC_ALL=C sort
  )
  [[ ${#shard_artifacts[@]} -gt 0 ]] || {
    echo "workflow run $run_id has no declared Rust shard performance receipts" >&2
    exit 1
  }
  for artifact in "${shard_artifacts[@]}"; do
    download_declared_artifact \
      "$artifact" "$evidence_dir/$artifact" \
      "$rust_shard_evidence_declaration"
  done
fi
download_declared_artifact \
  "medium-consumer-perf-receipt" \
  "$evidence_dir/medium-consumer" \
  'name: medium-consumer-perf-receipt'

# The repository workflow keeps every `needs` declaration on one line. Refuse a
# different shape instead of silently inventing dependencies for critical-path math.
awk '
  /^jobs:$/ { in_jobs = 1; next }
  in_jobs && /^[^ ]/ { exit }
  in_jobs && /^  [A-Za-z0-9_-]+:$/ {
    job = $1
    sub(/:$/, "", job)
    order[++count] = job
    needs[job] = ""
    next
  }
  in_jobs && job != "" && /^    needs:/ {
    value = $0
    sub(/^    needs:[[:space:]]*/, "", value)
    if (value == "") {
      print "multiline needs are not supported for job " job > "/dev/stderr"
      exit 2
    }
    gsub(/[\[\] ]/, "", value)
    needs[job] = value
  }
  END {
    for (position = 1; position <= count; position++) {
      current = order[position]
      printf "%s\t%s\n", current, needs[current]
    }
  }
' "$tmp_dir/workflow.yml" > "$tmp_dir/graph.tsv"
[[ -s "$tmp_dir/graph.tsv" ]] || {
  echo "workflow $workflow_path contains no jobs" >&2
  exit 1
}

mapfile -t producer_job_ids < <(
  jq -r '.jobs[] | select(.name == "producer-build") | .id' "$tmp_dir/jobs.json"
)
[[ ${#producer_job_ids[@]} -eq 1 ]] || {
  echo "workflow run $run_id must contain exactly one producer-build job" >&2
  exit 1
}
producer_log=$tmp_dir/producer-build.log
gh api --allow-escape-sequences \
  "repos/$repository/actions/jobs/${producer_job_ids[0]}/logs" > "$producer_log"
producer_cache_key=$(
  grep -Eo 'producer-bin-v1-[^[:space:]]+' "$producer_log" | head -1 || true
)
mapfile -t producer_key_digests < <(
  printf '%s\n' "$producer_cache_key" | grep -Eo '[0-9a-f]{64}' || true
)
[[ ${#producer_key_digests[@]} -eq 2 ]] || {
  echo "producer-build log does not expose one toolchain and one source cache digest" >&2
  exit 1
}
rustc_identity_sha256=${producer_key_digests[0]}
producer_source_identity_sha256=${producer_key_digests[1]}
runner_image=$(awk '
  / Image: / { image = $NF; found = 1; next }
  found && / Version: / { print image "@" $NF; exit }
' "$producer_log")
[[ -n "$runner_image" ]] || {
  echo "producer-build log does not identify the hosted runner image" >&2
  exit 1
}

# Count work from the completed job logs, not from elapsed time. `Compiling` rows are
# Cargo's actual unit-build events after every restored cache has been consulted. The
# command count separately identifies nextest test/archive build boundaries; it does not
# guess from how long a job happened to run.
job_logs=$tmp_dir/job-logs
mkdir -p "$job_logs"
while IFS=$'\t' read -r job_id job_name; do
  log=$job_logs/$job_id.log
  if [[ "$job_id" == "${producer_job_ids[0]}" ]]; then
    cp "$producer_log" "$log"
  else
    gh api --allow-escape-sequences \
      "repos/$repository/actions/jobs/$job_id/logs" > "$log"
  fi
  [[ -s "$log" ]] || {
    echo "job $job_name ($job_id) has no downloadable log" >&2
    exit 1
  }
done < <(jq -r '.jobs[] | [.id, .name] | @tsv' "$tmp_dir/jobs.json")

cargo_compilation_units=$(
  { grep -hE '(^|[[:space:]])Compiling[[:space:]]+[^[:space:]]+' "$job_logs"/*.log || true; } |
    wc -l | tr -d ' '
)
cargo_test_build_commands=$(
  {
    grep -hE \
      'cargo[[:space:]]+nextest[[:space:]]+(archive|run[^[:cntrl:]]*--no-run)' \
      "$job_logs"/*.log || true
  } | wc -l | tr -d ' '
)
if grep -q '^rust-prebuild'$'\t' "$tmp_dir/graph.tsv" &&
  grep -q '^rust-archive'$'\t' "$tmp_dir/graph.tsv"; then
  cargo_test_build_authorities=2
else
  cargo_test_build_authorities=$(
    jq '[.jobs[] | select(.name == "rust" or (.name | startswith("rust (")))] | length' \
      "$tmp_dir/jobs.json"
  )
fi

# Normalize matrix instances to the logical proof they execute. This is an
# inventory, not a scheduler diagram: moving a retained proof between a matrix
# branch and a standalone job keeps its identity, while a dropped baseline proof
# remains mechanically visible to the acceptance grader.
proof_inventory=$tmp_dir/proof-inventory.txt
jq -r '.jobs[].name' "$tmp_dir/jobs.json" |
  sed -E \
    -e 's/^generation \([^)]*\)$/generation/' \
    -e 's/^rust \([^)]*\)$/rust/' \
    -e 's/^heavy \(([^)]*)\)$/\1/' |
  LC_ALL=C sort -u > "$proof_inventory"
[[ -s "$proof_inventory" ]] || {
  echo "workflow run $run_id has no normalized proof inventory" >&2
  exit 1
}
proof_inventory_json=$(jq -R . < "$proof_inventory" | jq -s .)

job_block() {
  local wanted=$1
  awk -v wanted="$wanted" '
    $0 == "  " wanted ":" { inside = 1 }
    inside && $0 ~ /^  [A-Za-z0-9_-]+:$/ && $0 != "  " wanted ":" { exit }
    inside { print }
  ' "$tmp_dir/workflow.yml"
}

# These five required job groups invoke Make targets backed by GMEOW_DEV. Without
# an explicit artifact-backed override they each fall through to Make's source
# `cargo run -p gmeow-dev-cli` default. Count the authored fallback groups exactly;
# the counter is causal work, independent of runner timing or cache warmth.
producer_fallback_groups=$tmp_dir/producer-fallback-groups.txt
: > "$producer_fallback_groups"
for job in ontology-validate ontology-generated ontology-reason ontology-misc heavy; do
  grep -q "^${job}"$'\t' "$tmp_dir/graph.tsv" || {
    echo "workflow $workflow_path lacks required producer consumer job $job" >&2
    exit 1
  }
  block=$tmp_dir/job-$job.yml
  job_block "$job" > "$block"
  if ! grep -Eq '^[[:space:]]+GMEOW_DEV:|GMEOW_DEV=' "$block"; then
    echo "$job" >> "$producer_fallback_groups"
  fi
done
producer_fallback_job_groups=$(wc -l < "$producer_fallback_groups" | tr -d ' ')
producer_fallback_groups_json=$(jq -R . < "$producer_fallback_groups" | jq -s .)

workflow_sha256=$(sha256sum "$tmp_dir/workflow.yml" | cut -d' ' -f1)
job_graph_sha256=$(
  {
    printf 'gmeow:ci-job-graph:v1\0'
    cat "$tmp_dir/graph.tsv"
  } | sha256sum | cut -d' ' -f1
)

iso_ms() {
  local value=$1
  [[ "$value" != "null" && -n "$value" ]] || {
    echo 0
    return
  }
  date -u -d "$value" +%s%3N
}

declare -A duration_ms=()
declare -A job_seen=()
mapfile -t job_ids < <(cut -f1 "$tmp_dir/graph.tsv")
while IFS=$'\t' read -r name started completed conclusion _runner_name _runner_group _labels; do
  group=""
  for job_id in "${job_ids[@]}"; do
    if [[ "$name" == "$job_id" || "$name" == "$job_id ("* ]]; then
      group=$job_id
      break
    fi
  done
  [[ -n "$group" ]] || {
    echo "cannot bind Actions job name to workflow graph: $name" >&2
    exit 1
  }
  [[ "$conclusion" == "success" ]] || {
    echo "job $name did not succeed ($conclusion)" >&2
    exit 1
  }
  started_ms=$(iso_ms "$started")
  completed_ms=$(iso_ms "$completed")
  [[ $started_ms -gt 0 && $completed_ms -ge $started_ms ]] || {
    echo "job $name carries invalid start/completion timestamps" >&2
    exit 1
  }
  elapsed=$((completed_ms - started_ms))
  previous=${duration_ms[$group]:-0}
  if ((elapsed > previous)); then
    duration_ms[$group]=$elapsed
  fi
  job_seen[$group]=1
done < <(
  jq -r '.jobs[] | [
    .name,
    .started_at,
    .completed_at,
    .conclusion,
    (.runner_name // ""),
    (.runner_group_name // ""),
    ((.labels // []) | join(","))
  ] | @tsv' "$tmp_dir/jobs.json"
)

declare -A critical_to=()
critical_path_ms=0
critical_terminal=""
while IFS=$'\t' read -r job needs; do
  [[ ${job_seen[$job]:-0} -eq 1 ]] || {
    echo "successful run lacks workflow job $job" >&2
    exit 1
  }
  predecessor_max=0
  if [[ -n "$needs" ]]; then
    IFS=',' read -r -a predecessors <<< "$needs"
    for predecessor in "${predecessors[@]}"; do
      [[ -n ${critical_to[$predecessor]+x} ]] || {
        echo "job $job depends on unknown or non-topological predecessor $predecessor" >&2
        exit 1
      }
      if ((critical_to[$predecessor] > predecessor_max)); then
        predecessor_max=${critical_to[$predecessor]}
      fi
    done
  fi
  critical_to[$job]=$((predecessor_max + duration_ms[$job]))
  if ((critical_to[$job] > critical_path_ms)); then
    critical_path_ms=${critical_to[$job]}
    critical_terminal=$job
  fi
done < "$tmp_dir/graph.tsv"

graph_json=$tmp_dir/graph.json
jq -Rn '
  [inputs | split("\t") | {
    id: .[0],
    needs: (if (.[1] // "") == "" then [] else .[1] | split(",") end)
  }]
' < "$tmp_dir/graph.tsv" > "$graph_json"

created_ms=$(iso_ms "$(jq -r .created_at "$tmp_dir/run.json")")
run_started_ms=$(iso_ms "$(jq -r .run_started_at "$tmp_dir/run.json")")
updated_ms=$(iso_ms "$(jq -r .updated_at "$tmp_dir/run.json")")
queue_ms=$((run_started_ms - created_ms))
workflow_wall_ms=$((updated_ms - created_ms))

jobs_projection=$tmp_dir/jobs-projection.json
jq '[.jobs[] | {
  id, name, status, conclusion, started_at, completed_at,
  runner_name, runner_group_name, labels,
  steps: [.steps[]? | {name, status, conclusion, number, started_at, completed_at}]
}]' "$tmp_dir/jobs.json" > "$jobs_projection"
artifacts_projection=$tmp_dir/artifacts-projection.json
jq '[.artifacts[] | {
  id, name, size_in_bytes, expired, created_at, updated_at,
  archive_download_url,
  workflow_run: {id: .workflow_run.id, head_sha: .workflow_run.head_sha}
}]' "$tmp_dir/artifacts.json" > "$artifacts_projection"

evidence_jsonl=$tmp_dir/evidence.jsonl
: > "$evidence_jsonl"
while IFS= read -r -d '' file; do
  relative=${file#"$evidence_dir"/}
  jq -S -n \
    --arg path "$relative" \
    --arg sha256 "$(sha256sum "$file" | cut -d' ' -f1)" \
    --slurpfile payload "$file" \
    '{path: $path, sha256: $sha256, payload: $payload[0]}' >> "$evidence_jsonl"
done < <(find "$evidence_dir" -type f -name '*.json' -print0 | LC_ALL=C sort -z)
jq -s . "$evidence_jsonl" > "$tmp_dir/evidence.json"

# Canonical aggregate over the authenticated compact artifacts. Every row remains below
# `evidence`, so the aggregation is auditable and new schemas cannot silently disappear
# behind a scalar. Missing categories are zero only when the checked-in workflow did not
# declare their evidence artifact; a declared-but-missing artifact failed above.
jq -S \
  --argjson cargo_compilation_units "$cargo_compilation_units" \
  --argjson cargo_test_build_commands "$cargo_test_build_commands" \
  --argjson cargo_test_build_authorities "$cargo_test_build_authorities" '
  def payloads_starting($prefix):
    [.[] | select(.path | startswith($prefix)) | .payload];
  def payloads_containing($fragment):
    [.[] | select(.path | contains($fragment)) | .payload];
  def generation_updates:
    [.[] |
      select((.path | (startswith("generation-a/") or startswith("generation-b/"))) and
             (.path | endswith("update-timings.json"))) |
      .payload];
  def archive_rows: payloads_starting("rust-archive/");
  def fixture_rows:
    (archive_rows | map(select(.command == "prime-pipeline-test-fixtures"))) as $archive |
    if ($archive | length) > 0 then $archive
    else (payloads_starting("rust-prebuild/") |
      map(select(.command == "prime-pipeline-test-fixtures"))) end;
  def reason_rows: payloads_starting("reason/");
  def junit_rows:
    [.[] | select(.payload.command == "junit-inventory") | .payload];
  def perf_rows:
    [.[] | select(.payload.command == "perf-sample") | .payload];
  def archive_receipts:
    [archive_rows[] | select(.archive != null and .execution != null)];
  def number_sum(stream): [stream | numbers] | add // 0;

  generation_updates as $generation |
  fixture_rows as $fixtures |
  reason_rows as $reason |
  junit_rows as $junit |
  perf_rows as $perf |
  archive_receipts as $archives |
  {
    schema_version: 1,
    evidence: .,
    deterministic_work: {
      causal_counters: {
        actual_pipeline_stage_executions:
          ([$generation[].observations.stages[]? |
            select((.cached // false) != true)] | length),
        pipeline_stage_hydrations:
          ([$generation[].observations.stages[]? |
            select((.cached // false) == true)] | length),
        whole_dag_executions:
          number_sum($generation[] | (.observations.pipeline_runs // .pipeline_runs // 0)),
        fixture_builder_executions:
          ([$fixtures[].observations.fixtures[]? | select(.built == true)] | length),
        fixture_hydrations:
          ([$fixtures[].observations.fixtures[]? | select(.built == false)] | length),
        gts_import_requests:
          ([$fixtures[].observations.bundle_import? | select(. != null)] | length) +
          number_sum($reason[] | .deterministic_work.gts_imports // 0),
        gts_import_builds:
          ([$fixtures[].observations.bundle_import? |
            select(. != null and .built == true)] | length) +
          ([$reason[].observations | select(.gts_import_built == true)] | length),
        closure_constructions:
          ([$generation[].observations.stage_phases[]? |
            select(.phase == "construct-closure-and-artifacts")] | length) +
          number_sum($reason[] | .deterministic_work.closure_constructions // 0) +
          number_sum($reason[] |
            .deterministic_work.attestation.closure_constructions // 0),
        indexed_rdf_rows:
          number_sum($fixtures[].observations.bundle_import? |
            select(. != null and .built == true) | .receipt.dataset_quads) +
          number_sum($reason[] |
            select(.observations.gts_import_built == true) |
            .deterministic_work.gts_import.dataset_quads),
        fixture_output_rows:
          number_sum($fixtures[].observations.fixtures[]? |
            select(.built == true) | .receipt.dataset_quads),
        stage_cache_read_bytes:
          number_sum($generation[].observations.stages[]?.cache_read_bytes),
        stage_cache_write_bytes:
          number_sum($generation[].observations.stages[]?.cache_write_bytes),
        fixture_transfer_bytes:
          number_sum($fixtures[].observations.fixtures[]?.transferred_bytes) +
          number_sum($fixtures[].observations.bundle_import?.transferred_bytes),
        generated_output_bytes:
          number_sum($generation[] | .deterministic_work.managed_output_bytes),
        nextest_archive_builds: ($archives | length),
        nextest_archive_bytes: number_sum($archives[] | .archive.bytes),
        cargo_compilation_units: $cargo_compilation_units,
        cargo_test_build_commands: $cargo_test_build_commands,
        cargo_test_build_authorities: $cargo_test_build_authorities
      },
      nextest_inventory: (
        if ($archives | length) == 1 then {
          sha256: $archives[0].execution.inventory_sha256,
          count: $archives[0].execution.inventory_count,
          profile: $archives[0].execution.profile,
          partition_scheme: $archives[0].execution.partition_scheme,
          partition_count: $archives[0].execution.partition_count
        } else null end
      ),
      junit: {
        shard_count: ($junit | length),
        test_count: number_sum($junit[] | .deterministic_work.test_count),
        inventory_digests: ([$junit[].deterministic_work.inventory_sha256] | sort)
      }
    },
    observations: {
      pipeline_cache_hydration_rss_delta_kib_max:
        ([$generation[].observations.stages[]?.cache_hydration_rss_delta_kib |
          numbers] | max // 0),
      sampled_command_wall_ms: number_sum($perf[] | .observations.wall_ms),
      sampled_user_cpu_ms:
        number_sum($perf[] | .observations.resource_usage.user_cpu_ms),
      sampled_system_cpu_ms:
        number_sum($perf[] | .observations.resource_usage.system_cpu_ms),
      sampled_max_rss_kib:
        ([$perf[].observations.resource_usage.max_rss_kib | numbers] | max // 0),
      junit_duration_micros:
        number_sum($junit[] | .observations.duration_micros),
      archive_sample_count: ($perf | length)
    }
  }
  ' "$tmp_dir/evidence.json" > "$tmp_dir/work-evidence.json"

mkdir -p "$(dirname "$output")"
candidate=$tmp_dir/receipt.json
jq -S -n \
  --argjson schema_version 3 \
  --argjson run_id "$run_id" \
  --argjson run_attempt "$(jq -r .run_attempt "$tmp_dir/run.json")" \
  --arg pair_id "$pair_id" \
  --arg variant "$variant" \
  --argjson sample_index "$sample_index" \
  --arg node_class "$node_class" \
  --arg cache_state "$cache_state" \
  --argjson cache_classes "$cache_classes" \
  --arg partial_change "$partial_change" \
  --arg runner_image "$runner_image" \
  --arg rustc_identity_sha256 "$rustc_identity_sha256" \
  --arg producer_source_identity_sha256 "$producer_source_identity_sha256" \
  --arg repository "$repository" \
  --arg head_sha "$head_sha" \
  --arg artifact_sha "$artifact_sha" \
  --arg head_branch "$(jq -r .head_branch "$tmp_dir/run.json")" \
  --arg event "$(jq -r .event "$tmp_dir/run.json")" \
  --arg workflow_path "$workflow_path" \
  --arg workflow_sha256 "$workflow_sha256" \
  --arg job_graph_sha256 "$job_graph_sha256" \
  --arg critical_terminal "$critical_terminal" \
  --arg html_url "$(jq -r .html_url "$tmp_dir/run.json")" \
  --argjson created_unix_ms "$created_ms" \
  --argjson queue_ms "$queue_ms" \
  --argjson workflow_wall_ms "$workflow_wall_ms" \
  --argjson critical_path_execution_ms "$critical_path_ms" \
  --argjson graph "$(cat "$graph_json")" \
  --argjson proof_inventory "$proof_inventory_json" \
  --argjson producer_fallback_job_groups "$producer_fallback_job_groups" \
  --argjson producer_fallback_groups "$producer_fallback_groups_json" \
  --slurpfile work_evidence_file "$tmp_dir/work-evidence.json" \
  --argjson jobs "$(cat "$jobs_projection")" \
  --argjson artifacts "$(cat "$artifacts_projection")" \
  '$work_evidence_file[0] as $work_evidence | {
    schema_version: $schema_version,
    command: "ci-run-receipt",
    sample_identity: {
      run_id: $run_id,
      run_attempt: $run_attempt,
      pair_id: $pair_id,
      variant: $variant,
      sample_index: $sample_index,
      node_class: $node_class,
      cache_state: $cache_state,
      cache_classes: $cache_classes,
      partial_change: (if $partial_change == "" then null else $partial_change end),
      runner_image: $runner_image,
      rustc_identity_sha256: $rustc_identity_sha256,
      measured_command: ["github-actions", $workflow_path],
      repository: $repository,
      head_sha: $head_sha,
      tested_artifact_sha: $artifact_sha,
      head_branch: $head_branch,
      event: $event,
      workflow_path: $workflow_path,
      workflow_sha256: $workflow_sha256,
      job_graph_sha256: $job_graph_sha256,
      html_url: $html_url
    },
    deterministic_work: {
      job_graph_sha256: $job_graph_sha256,
      producer_source_identity_sha256: $producer_source_identity_sha256,
      jobs: $graph,
      proof_inventory: $proof_inventory,
      causal_counters: ($work_evidence.deterministic_work.causal_counters + {
        source_producer_fallback_job_groups: $producer_fallback_job_groups
      }),
      nextest_inventory: $work_evidence.deterministic_work.nextest_inventory,
      junit: $work_evidence.deterministic_work.junit,
      source_producer_fallback_groups: $producer_fallback_groups
    },
    observations: {
      conclusion: "success",
      created_unix_ms: $created_unix_ms,
      queue_ms: $queue_ms,
      workflow_wall_ms: $workflow_wall_ms,
      critical_path_execution_ms: $critical_path_execution_ms,
      critical_terminal: $critical_terminal,
      causal_work_observations: $work_evidence.observations,
      jobs: $jobs,
      artifacts: $artifacts
    },
    run_telemetry: $work_evidence
  }' > "$candidate"
mv "$candidate" "$output"
echo "$output"
