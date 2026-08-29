#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# The lint target checks common.sh independently and does not enable
# external-source traversal for this runtime-resolved path.
# shellcheck disable=SC1091
source "$script_dir/../build-support/common.sh"

usage() {
  cat >&2 << 'EOF'
usage:
  producer-receipt.sh write <generated-a> <manifest-a> <generated-b> <manifest-b> <receipt> <source-sha>
  producer-receipt.sh verify <generated> <receipt>
EOF
  exit 2
}

[[ $# -ge 1 ]] || usage
mode=$1
shift
command -v jq > /dev/null || {
  echo "jq is required to verify producer receipts" >&2
  exit 1
}
repo_root=$(repository_root)
cd "$repo_root"
tmp_dir=$(mktemp -d)
trap 'rm -rf -- "$tmp_dir"' EXIT

tree_digest() {
  local root=$1
  [[ -d "$root" ]] || {
    echo "tree does not exist: $root" >&2
    return 1
  }
  local paths
  local manifest
  paths=$(mktemp "$tmp_dir/tree-paths.XXXXXX")
  manifest=$(mktemp "$tmp_dir/tree-manifest.XXXXXX")
  (
    cd "$root"
    find . \( -type f -o -type l \) -print0 | LC_ALL=C sort -z > "$paths"
    : > "$manifest"
    while IFS= read -r -d '' path; do
      local_path=${path#./}
      if [[ -L "$local_path" ]]; then
        link_target=$(readlink -- "$local_path")
        printf 'symlink\0%s\0%s\0' "$local_path" "$link_target" >> "$manifest"
      elif [[ -f "$local_path" ]]; then
        file_digest=$(sha256sum -- "$local_path" | cut -d' ' -f1)
        printf 'file\0%s\0%s\0' "$local_path" "$file_digest" >> "$manifest"
      else
        echo "tree entry disappeared or changed type while hashing: $root/$local_path" >&2
        return 1
      fi
    done < "$paths"
  )
  sha256sum "$manifest" | cut -d' ' -f1
}

tree_bytes() {
  local root=$1
  find "$root" -type f -printf '%s\n' |
    awk '{ total += $1 } END { printf "%.0f\n", total + 0 }'
}

tracked_tree_digest() {
  local paths
  local manifest
  paths=$(mktemp "$tmp_dir/source-paths.XXXXXX")
  manifest=$(mktemp "$tmp_dir/source-manifest.XXXXXX")
  git ls-files --cached --others --exclude-standard -z | LC_ALL=C sort -z > "$paths"
  : > "$manifest"
  while IFS= read -r -d '' path; do
    if [[ -L "$path" ]]; then
      link_target=$(readlink -- "$path")
      file_mode=$(stat -c '%a' -- "$path")
      printf 'symlink\0%s\0%s\0%s\0' "$path" "$file_mode" "$link_target" >> "$manifest"
    elif [[ -f "$path" ]]; then
      file_mode=$(stat -c '%a' -- "$path")
      file_digest=$(sha256sum -- "$path" | cut -d' ' -f1)
      printf 'file\0%s\0%s\0%s\0' "$path" "$file_mode" "$file_digest" >> "$manifest"
    elif [[ ! -e "$path" ]]; then
      printf 'missing\0%s\0' "$path" >> "$manifest"
    else
      echo "source entry has unsupported type: $path" >&2
      return 1
    fi
  done < "$paths"
  sha256sum "$manifest" | cut -d' ' -f1
}

normalized_manifest() {
  jq -S '{
    version,
    build_fingerprint,
    build_identity,
    input_digest,
    output,
    language,
    strict_checked,
    materialized,
    docs_rendered,
    managed_roots,
    files: ([.files[] | {path, len, sha256}] | sort_by(.path)),
    managed_output_root,
    stage_receipt_root
  }' "$1"
}

verify_manifest_tree() {
  local generated=$1
  local manifest=$2
  local expected=$tmp_dir/expected-generated.tsv
  local actual=$tmp_dir/actual-generated.tsv
  local paths
  paths=$(mktemp "$tmp_dir/generated-paths.XXXXXX")
  jq -r '
    .files[] |
    select(.path == "generated" or (.path | startswith("generated/"))) |
    [.path, (.len | tostring), .sha256] | @tsv
  ' "$manifest" | LC_ALL=C sort > "$expected"
  (
    cd "$(dirname "$generated")"
    tree_name=$(basename "$generated")
    find "$tree_name" -type f -print0 | LC_ALL=C sort -z > "$paths"
    while IFS= read -r -d '' path; do
      relative=${path#./}
      bytes=$(stat -c '%s' "$relative")
      digest=$(sha256sum "$relative" | cut -d' ' -f1)
      printf 'generated/%s\t%s\t%s\n' "${relative#"$tree_name/"}" "$bytes" "$digest"
    done < "$paths"
  ) > "$actual"
  if ! cmp -s "$expected" "$actual"; then
    echo "generated tree differs from its sync manifest" >&2
    diff -u "$expected" "$actual" >&2 || true
    return 1
  fi
}

validate_manifest_contract() {
  local manifest=$1
  # Kept in lockstep with dev_sync::MANIFEST_VERSION by make_gate_contract.
  jq -e '
    .version == 5 and
    .output == "generated" and
    .language == "default" and
    .strict_checked == true and
    .materialized == true and
    (.build_fingerprint | test("^[0-9a-f]{64}$")) and
    (.managed_output_root | test("^[0-9a-f]{64}$")) and
    (.stage_receipt_root | test("^[0-9a-f]{64}$")) and
    (.build_identity.fingerprint == .build_fingerprint) and
    (.files | length > 0)
  ' "$manifest" > /dev/null || {
    echo "sync manifest does not satisfy the producer receipt contract: $manifest" >&2
    return 1
  }
}

envelope_digest() {
  local payload=$1
  {
    printf 'gmeow:producer-receipt:v1\0'
    cat "$payload"
  } | sha256sum | cut -d' ' -f1
}

validate_receipt_contract() {
  local receipt=$1
  [[ $(stat -c '%s' "$receipt") -le 4194304 ]] || {
    echo "producer receipt exceeds the 4 MiB structural bound: $receipt" >&2
    return 1
  }
  if ! jq -e '
    .receipt_digest | type == "string" and test("^[0-9a-f]{64}$")
  ' "$receipt" > /dev/null; then
    echo "producer receipt has an unknown, missing, or malformed field: $receipt" >&2
    return 1
  fi
  if ! jq -e '
    .payload.schema_version == 1 and
    (.payload.source_sha | type == "string" and test("^[0-9a-f]{40}$")) and
    (.payload.source_tree_sha256 | type == "string" and test("^[0-9a-f]{64}$")) and
    (.payload.build_identity.fingerprint | type == "string" and test("^[0-9a-f]{64}$")) and
    (.payload.sync_input_digest | type == "string" and test("^[0-9a-f]{64}$")) and
    (.payload.stage_receipt_root | type == "string" and test("^[0-9a-f]{64}$")) and
    (.payload.managed_outputs.root | type == "string" and test("^[0-9a-f]{64}$")) and
    (.payload.managed_outputs.manifest_sha256 | type == "string" and test("^[0-9a-f]{64}$")) and
    (.payload.managed_outputs.file_count | type == "number" and . > 0) and
    (.payload.managed_outputs.bytes | type == "number" and . > 0) and
    (.payload.generated.tree_sha256 | type == "string" and test("^[0-9a-f]{64}$")) and
    (.payload.generated.file_count | type == "number" and . > 0) and
    (.payload.generated.bytes | type == "number" and . > 0) and
    (.payload.generated.bundle_sha256 | type == "string" and test("^[0-9a-f]{64}$")) and
    (.payload.generated.bundle_bytes | type == "number" and . > 0)
  ' "$receipt" > /dev/null; then
    echo "producer receipt has an unknown, missing, or malformed field: $receipt" >&2
    return 1
  fi
}

if [[ "$mode" == "write" ]]; then
  [[ $# -eq 6 ]] || usage
  generated_a=$(realpath "$1")
  manifest_a=$(realpath "$2")
  generated_b=$(realpath "$3")
  manifest_b=$(realpath "$4")
  receipt=$(realpath -m "$5")
  expected_source_sha=$6
  actual_source_sha=$(git rev-parse HEAD)
  [[ "$actual_source_sha" == "$expected_source_sha" ]] || {
    echo "source SHA $actual_source_sha != requested $expected_source_sha" >&2
    exit 1
  }
  validate_manifest_contract "$manifest_a"
  validate_manifest_contract "$manifest_b"
  verify_manifest_tree "$generated_a" "$manifest_a"
  verify_manifest_tree "$generated_b" "$manifest_b"

  normalized_a=$tmp_dir/manifest-a.json
  normalized_b=$tmp_dir/manifest-b.json
  normalized_manifest "$manifest_a" > "$normalized_a"
  normalized_manifest "$manifest_b" > "$normalized_b"
  if ! cmp -s "$normalized_a" "$normalized_b"; then
    echo "independent generation manifests differ" >&2
    diff -u "$normalized_a" "$normalized_b" >&2 || true
    exit 1
  fi
  generated_a_sha256=$(tree_digest "$generated_a")
  generated_b_sha256=$(tree_digest "$generated_b")
  [[ "$generated_a_sha256" == "$generated_b_sha256" ]] || {
    echo "independent generated tree roots differ" >&2
    exit 1
  }

  payload=$tmp_dir/payload.json
  source_tree_sha256=$(tracked_tree_digest)
  manifest_sha256=$(sha256sum "$normalized_a" | cut -d' ' -f1)
  generated_file_count=$(find "$generated_a" -type f | wc -l | tr -d ' ')
  generated_bytes=$(tree_bytes "$generated_a")
  bundle=$generated_a/dist/gmeow.gts
  bundle_sha256=$(sha256sum "$bundle" | cut -d' ' -f1)
  bundle_bytes=$(stat -c '%s' "$bundle")
  managed_file_count=$(jq '.files | length' "$normalized_a")
  managed_bytes=$(jq '[.files[].len] | add' "$normalized_a")

  jq -S -n \
    --argjson schema_version 1 \
    --arg source_sha "$actual_source_sha" \
    --arg source_tree_sha256 "$source_tree_sha256" \
    --argjson build_identity "$(jq '.build_identity' "$normalized_a")" \
    --arg sync_input_digest "$(jq -r '.input_digest' "$normalized_a")" \
    --arg stage_receipt_root "$(jq -r '.stage_receipt_root' "$normalized_a")" \
    --arg managed_output_root "$(jq -r '.managed_output_root' "$normalized_a")" \
    --arg managed_manifest_sha256 "$manifest_sha256" \
    --argjson managed_file_count "$managed_file_count" \
    --argjson managed_bytes "$managed_bytes" \
    --arg generated_tree_sha256 "$generated_a_sha256" \
    --argjson generated_file_count "$generated_file_count" \
    --argjson generated_bytes "$generated_bytes" \
    --arg bundle_sha256 "$bundle_sha256" \
    --argjson bundle_bytes "$bundle_bytes" \
    '{
      schema_version: $schema_version,
      source_sha: $source_sha,
      source_tree_sha256: $source_tree_sha256,
      build_identity: $build_identity,
      sync_input_digest: $sync_input_digest,
      stage_receipt_root: $stage_receipt_root,
      managed_outputs: {
        root: $managed_output_root,
        manifest_sha256: $managed_manifest_sha256,
        file_count: $managed_file_count,
        bytes: $managed_bytes
      },
      generated: {
        tree_sha256: $generated_tree_sha256,
        file_count: $generated_file_count,
        bytes: $generated_bytes,
        bundle_sha256: $bundle_sha256,
        bundle_bytes: $bundle_bytes
      }
    }' > "$payload"
  digest=$(envelope_digest "$payload")
  mkdir -p "$(dirname "$receipt")"
  jq -S -n --arg receipt_digest "$digest" --argjson payload "$(cat "$payload")" \
    '{receipt_digest: $receipt_digest, payload: $payload}' > "$receipt"
  validate_receipt_contract "$receipt"
  echo "producer receipt written: $receipt"
elif [[ "$mode" == "verify" ]]; then
  [[ $# -eq 2 ]] || usage
  generated=$(realpath "$1")
  receipt=$(realpath "$2")
  validate_receipt_contract "$receipt"
  payload=$tmp_dir/payload.json
  jq -S '.payload' "$receipt" > "$payload"
  expected_digest=$(jq -r '.receipt_digest' "$receipt")
  actual_digest=$(envelope_digest "$payload")
  [[ "$actual_digest" == "$expected_digest" ]] || {
    echo "producer receipt envelope digest mismatch" >&2
    exit 1
  }
  [[ "$(git rev-parse HEAD)" == "$(jq -r '.source_sha' "$payload")" ]] || {
    echo "producer receipt source SHA mismatch" >&2
    exit 1
  }
  [[ "$(tracked_tree_digest)" == "$(jq -r '.source_tree_sha256' "$payload")" ]] || {
    echo "producer receipt tracked source tree mismatch" >&2
    exit 1
  }
  [[ "$(tree_digest "$generated")" == "$(jq -r '.generated.tree_sha256' "$payload")" ]] || {
    echo "producer receipt generated tree mismatch" >&2
    exit 1
  }
  [[ "$(find "$generated" -type f | wc -l | tr -d ' ')" == "$(jq -r '.generated.file_count' "$payload")" ]] || {
    echo "producer receipt generated file-count mismatch" >&2
    exit 1
  }
  [[ "$(tree_bytes "$generated")" == "$(jq -r '.generated.bytes' "$payload")" ]] || {
    echo "producer receipt generated byte-count mismatch" >&2
    exit 1
  }
  bundle=$generated/dist/gmeow.gts
  [[ "$(sha256sum "$bundle" | cut -d' ' -f1)" == "$(jq -r '.generated.bundle_sha256' "$payload")" ]] || {
    echo "producer receipt bundle digest mismatch" >&2
    exit 1
  }
  [[ "$(stat -c '%s' "$bundle")" == "$(jq -r '.generated.bundle_bytes' "$payload")" ]] || {
    echo "producer receipt bundle byte-count mismatch" >&2
    exit 1
  }
  echo "producer receipt verified: $actual_digest"
else
  usage
fi
