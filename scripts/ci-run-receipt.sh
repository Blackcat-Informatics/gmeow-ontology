#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Capture one completed GitHub Actions run as performance evidence. This is a
# report-only collector: it never changes a workflow run or grades correctness.

set -euo pipefail

usage() {
  echo "usage: $0 <run-id> <baseline|candidate> <sample-index> <node-class> <output.json>" >&2
  exit 2
}

[[ $# -eq 5 ]] || usage
run_id=$1
variant=$2
sample_index=$3
node_class=$4
output=$5
[[ "$run_id" =~ ^[1-9][0-9]*$ ]] || usage
[[ "$variant" == "baseline" || "$variant" == "candidate" ]] || usage
[[ "$sample_index" =~ ^[1-9][0-9]*$ ]] || usage
[[ -n "$node_class" ]] || usage

for tool in gh jq sha256sum base64 date awk; do
  command -v "$tool" >/dev/null || {
    echo "$tool is required to collect CI run evidence" >&2
    exit 1
  }
done

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
workflow_path=$(jq -r .path "$tmp_dir/run.json")
workflow_path=${workflow_path%%@*}
workflow_path=${workflow_path#./}
gh api "repos/$repository/contents/$workflow_path?ref=$head_sha" --jq .content |
  tr -d '\n' | base64 --decode > "$tmp_dir/workflow.yml"

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

mkdir -p "$(dirname "$output")"
candidate=$tmp_dir/receipt.json
jq -S -n \
  --argjson schema_version 1 \
  --argjson run_id "$run_id" \
  --arg variant "$variant" \
  --argjson sample_index "$sample_index" \
  --arg node_class "$node_class" \
  --arg repository "$repository" \
  --arg head_sha "$head_sha" \
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
  --argjson jobs "$(cat "$jobs_projection")" \
  --argjson artifacts "$(cat "$artifacts_projection")" \
  '{
    schema_version: $schema_version,
    command: "ci-run-receipt",
    sample_identity: {
      run_id: $run_id,
      variant: $variant,
      sample_index: $sample_index,
      node_class: $node_class,
      repository: $repository,
      head_sha: $head_sha,
      head_branch: $head_branch,
      event: $event,
      workflow_path: $workflow_path,
      workflow_sha256: $workflow_sha256,
      job_graph_sha256: $job_graph_sha256,
      html_url: $html_url
    },
    deterministic_work: {
      job_graph_sha256: $job_graph_sha256,
      jobs: $graph
    },
    observations: {
      created_unix_ms: $created_unix_ms,
      queue_ms: $queue_ms,
      workflow_wall_ms: $workflow_wall_ms,
      critical_path_execution_ms: $critical_path_execution_ms,
      critical_terminal: $critical_terminal,
      jobs: $jobs,
      artifacts: $artifacts
    }
  }' > "$candidate"
mv "$candidate" "$output"
echo "$output"
