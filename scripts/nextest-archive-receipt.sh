#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

usage() {
  echo "usage: $0 <write|verify> <archive.tar.zst> <receipt.json> <shards> <nextest-version>" >&2
  exit 2
}

[[ $# -eq 5 ]] || usage
mode=$1
archive=$2
receipt=$3
shards=$4
expected_nextest=$5
[[ "$mode" == "write" || "$mode" == "verify" ]] || usage
[[ "$shards" =~ ^[1-9][0-9]*$ ]] || {
  echo "shards must be a positive integer, got: $shards" >&2
  exit 2
}
[[ -f "$archive" ]] || {
  echo "nextest archive does not exist: $archive" >&2
  exit 1
}
command -v jq >/dev/null || {
  echo "jq is required to authenticate the nextest inventory" >&2
  exit 1
}

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"
junit_inventory=$repo_root/dist/nextest/junit_inventory
perf_sample=$repo_root/dist/nextest/perf_sample
perf_accept=$repo_root/dist/nextest/perf_accept
test_fixture_manifest=$repo_root/.cache/gmeow-sync/test-fixture-manifest-v2.json
for evidence_tool in "$junit_inventory" "$perf_sample" "$perf_accept"; do
  [[ -x "$evidence_tool" ]] || {
    echo "nextest evidence tool does not exist or is not executable: $evidence_tool" >&2
    exit 1
  }
done
[[ -f "$test_fixture_manifest" ]] || {
  echo "test-fixture manifest does not exist: $test_fixture_manifest" >&2
  exit 1
}
archive=$(realpath "$archive")
receipt=$(realpath -m "$receipt")
nextest_release=$(cargo nextest --version | sed -n 's/^release: //p')
[[ "$nextest_release" == "$expected_nextest" ]] || {
  echo "cargo-nextest release $nextest_release != pinned $expected_nextest" >&2
  exit 1
}

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
      # A tracked deletion is a real working-tree input while an issue branch is
      # being validated before commit. Bind the absence instead of silently
      # authenticating the index's former bytes.
      printf 'missing\0%s\0' "$path" >> "$manifest"
    else
      echo "source entry has unsupported type: $path" >&2
      return 1
    fi
  done < "$paths"
  sha256sum "$manifest" | cut -d' ' -f1
}

inventory() {
  cargo nextest list \
    --archive-file "$archive" \
    --workspace-remap "$repo_root" \
    --profile ci \
    "$@" \
    --message-format json |
    jq -r '
      ."rust-suites" | to_entries[] as $suite |
      $suite.value.testcases | to_entries[] |
      select(.value."filter-match".status == "matches") |
      [$suite.value."package-name", $suite.value."binary-id", .key] | @tsv
    ' | LC_ALL=C sort
}

prove_partitions() {
  local canonical=$1
  local union=$tmp_dir/partition-union.tsv
  : > "$union"
  for ((index = 1; index <= shards; index++)); do
    part=$tmp_dir/partition-$index.tsv
    inventory --partition "slice:$index/$shards" > "$part"
    if [[ -s "$part" ]] && [[ -n "$(uniq -d "$part")" ]]; then
      echo "nextest slice $index/$shards contains duplicate test identities" >&2
      uniq -d "$part" >&2
      return 1
    fi
    cat "$part" >> "$union"
  done
  LC_ALL=C sort "$union" -o "$union"
  if [[ -n "$(uniq -d "$union")" ]]; then
    echo "nextest slice partitions overlap" >&2
    uniq -d "$union" >&2
    return 1
  fi
  if ! cmp -s "$canonical" "$union"; then
    echo "nextest slice union differs from the canonical CI-profile inventory" >&2
    comm -3 "$canonical" "$union" | head -100 >&2
    return 1
  fi
}

write_candidate() {
  local output=$1
  canonical=$tmp_dir/canonical.tsv
  inventory > "$canonical"
  [[ -s "$canonical" ]] || {
    echo "nextest archive selected zero CI-profile tests" >&2
    return 1
  }
  if [[ -n "$(uniq -d "$canonical")" ]]; then
    echo "canonical nextest inventory contains duplicate identities" >&2
    uniq -d "$canonical" >&2
    return 1
  fi
  prove_partitions "$canonical"

  source_sha=$(git rev-parse HEAD)
  source_tree_sha256=$(tracked_tree_digest)
  rustc_sha256=$(rustc -Vv | sha256sum | cut -d' ' -f1)
  nextest_identity=$(cargo nextest --version | sha256sum | cut -d' ' -f1)
  generated_sha256=$(tree_digest generated)
  archive_sha256=$(sha256sum "$archive" | cut -d' ' -f1)
  archive_bytes=$(stat -c '%s' "$archive")
  junit_inventory_sha256=$(sha256sum "$junit_inventory" | cut -d' ' -f1)
  junit_inventory_bytes=$(stat -c '%s' "$junit_inventory")
  perf_sample_sha256=$(sha256sum "$perf_sample" | cut -d' ' -f1)
  perf_sample_bytes=$(stat -c '%s' "$perf_sample")
  perf_accept_sha256=$(sha256sum "$perf_accept" | cut -d' ' -f1)
  perf_accept_bytes=$(stat -c '%s' "$perf_accept")
  test_fixture_manifest_sha256=$(sha256sum "$test_fixture_manifest" | cut -d' ' -f1)
  test_fixture_manifest_bytes=$(stat -c '%s' "$test_fixture_manifest")
  inventory_sha256=$(sha256sum "$canonical" | cut -d' ' -f1)
  inventory_count=$(wc -l < "$canonical" | tr -d ' ')
  build_config_sha256=$(
    {
      printf 'profile=ci\0nextest=%s\0rustflags=%s\0cflags=%s\0cxxflags=%s\0' \
        "$expected_nextest" \
        "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS:-}" \
        "${CARGO_ENV_CFLAGS_VALUE:-}" \
        "${CARGO_ENV_CXXFLAGS_VALUE:-}"
      sha256sum Cargo.toml Cargo.lock rust-toolchain.toml .cargo/config.toml .config/nextest.toml
    } | sha256sum | cut -d' ' -f1
  )

  mkdir -p "$(dirname "$output")"
  jq -S -n \
    --argjson schema_version 3 \
    --arg source_sha "$source_sha" \
    --arg source_tree_sha256 "$source_tree_sha256" \
    --arg rustc_identity_sha256 "$rustc_sha256" \
    --arg nextest_version "$expected_nextest" \
    --arg nextest_identity_sha256 "$nextest_identity" \
    --arg generated_tree_sha256 "$generated_sha256" \
    --arg build_config_sha256 "$build_config_sha256" \
    --arg archive_file "$(basename "$archive")" \
    --arg archive_sha256 "$archive_sha256" \
    --argjson archive_bytes "$archive_bytes" \
    --arg junit_inventory_file "$(basename "$junit_inventory")" \
    --arg junit_inventory_sha256 "$junit_inventory_sha256" \
    --argjson junit_inventory_bytes "$junit_inventory_bytes" \
    --arg perf_sample_file "$(basename "$perf_sample")" \
    --arg perf_sample_sha256 "$perf_sample_sha256" \
    --argjson perf_sample_bytes "$perf_sample_bytes" \
    --arg perf_accept_file "$(basename "$perf_accept")" \
    --arg perf_accept_sha256 "$perf_accept_sha256" \
    --argjson perf_accept_bytes "$perf_accept_bytes" \
    --arg test_fixture_manifest_file ".cache/gmeow-sync/test-fixture-manifest-v2.json" \
    --arg test_fixture_manifest_sha256 "$test_fixture_manifest_sha256" \
    --argjson test_fixture_manifest_bytes "$test_fixture_manifest_bytes" \
    --arg profile ci \
    --arg partition_scheme slice \
    --argjson partition_count "$shards" \
    --arg inventory_sha256 "$inventory_sha256" \
    --argjson inventory_count "$inventory_count" \
    '{
      schema_version: $schema_version,
      source_sha: $source_sha,
      source_tree_sha256: $source_tree_sha256,
      rustc_identity_sha256: $rustc_identity_sha256,
      nextest_version: $nextest_version,
      nextest_identity_sha256: $nextest_identity_sha256,
      generated_tree_sha256: $generated_tree_sha256,
      build_config_sha256: $build_config_sha256,
      archive: {
        file: $archive_file,
        sha256: $archive_sha256,
        bytes: $archive_bytes
      },
      evidence_tools: {
        junit_inventory: {
          file: $junit_inventory_file,
          sha256: $junit_inventory_sha256,
          bytes: $junit_inventory_bytes
        },
        perf_sample: {
          file: $perf_sample_file,
          sha256: $perf_sample_sha256,
          bytes: $perf_sample_bytes
        },
        perf_accept: {
          file: $perf_accept_file,
          sha256: $perf_accept_sha256,
          bytes: $perf_accept_bytes
        }
      },
      test_fixture_manifest: {
        file: $test_fixture_manifest_file,
        sha256: $test_fixture_manifest_sha256,
        bytes: $test_fixture_manifest_bytes
      },
      execution: {
        profile: $profile,
        partition_scheme: $partition_scheme,
        partition_count: $partition_count,
        inventory_sha256: $inventory_sha256,
        inventory_count: $inventory_count
      }
    }' > "$output"
}

if [[ "$mode" == "write" ]]; then
  write_candidate "$receipt"
  echo "nextest archive receipt written: $receipt"
else
  [[ -f "$receipt" ]] || {
    echo "nextest archive receipt does not exist: $receipt" >&2
    exit 1
  }
  candidate=$tmp_dir/receipt.json
  write_candidate "$candidate"
  if ! cmp -s "$receipt" "$candidate"; then
    echo "nextest archive receipt mismatch" >&2
    diff -u "$receipt" "$candidate" >&2 || true
    exit 1
  fi
  echo "nextest archive receipt verified: $(jq -r '.archive.sha256' "$receipt")"
fi
