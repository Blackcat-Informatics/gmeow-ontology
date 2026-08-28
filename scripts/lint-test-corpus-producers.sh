#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Fail before any test process starts if test code can reach a repository-corpus
# producer. Tests may consume authenticated products; only an explicit, prior DAG
# producer stage may build them.

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

producer_calls='(run_full(_scoped)?|run_import|run_targets|run_acceptance(_corpus)?|AcceptanceContext::load|prime_stage_fixtures?|produce_repository_verdict|repository::execute_worker|run_slice_spec_worker|validate_flagship_manifest|run_(competency|structural|conformance)_file|merged_store|rdfs_closed_store|native_closed_store|import_graph_preserving_cached|core_browser_bundle_nquads|compile_mappings|compile_statements|load_authored[A-Za-z0-9_]*|examples_graph|build_[A-Za-z0-9_]*corpus|build_substrate_(projection|sbom_projection)|project_bundle|project_decoded_bundle|check_superset|project_four_codecs|fold_release_bundle|build_coherence_evidence|render_result_shapes|DocsModel::discover|replay_runtime_store|emit_gts|emit_gmeow_gts(_with_medium)?|dataset_to_gmeow_gts|gts_write::to_gts|gmeow_music::piece_to_gts_bytes|gmeow_math::turtle_to_gts|GmeowGtsWriter::new|e8_weyl_order|additive_he_demo|proof_ingest|exact_pca_residual|probability_model_seam|pvalue_tri_slice|clifford_twelve_thirteen|r_lift|onnx_lift|proof_lift|identity_rewrite|flipped_payload_byte|unknown_dictionary_id|undeclared_dictionary|unregistered_rep|undecodable_payload|snapshot_dataset|serialize_carrier_snapshot(_with_receipt)?|render_(apache|bench_leaderboard|catalog|constraint_catalog|constraint_shapes|cost_ledger|docs_graph|evals|frame_shapes|matrix|profiles|pydantic_package|references|research_objects|result_shapes|schemas|skos_surface|soak|term_manifest)|load_or_build|load_or_build_with_identity|try_load_or_build|try_load_or_build_with_identity|model_identity_or_build|shape_union::(load_shapes|shape_files)|data_graph_shapes_from_gts|shapes_from_gts_excluding)[[:space:]]*\('
# These entry points discover or assemble repository-owned source data. A nearby
# synthetic marker can never suppress them. The marker is only meaningful for a
# pure transformation over explicit tiny input (for example, one in-memory GTS frame).
repository_bound_calls='(run_full(_scoped)?|run_import|run_acceptance(_corpus)?|AcceptanceContext::load|prime_stage_fixtures?|produce_repository_verdict|repository::execute_worker|run_slice_spec_worker|validate_flagship_manifest|run_(competency|structural|conformance)_file|merged_store|rdfs_closed_store|native_closed_store|compile_mappings|compile_statements|load_authored[A-Za-z0-9_]*|examples_graph|build_[A-Za-z0-9_]*corpus|build_substrate_(projection|sbom_projection)|check_superset|DocsModel::discover|replay_runtime_store|e8_weyl_order|additive_he_demo|proof_ingest|exact_pca_residual|probability_model_seam|pvalue_tri_slice|clifford_twelve_thirteen|r_lift|onnx_lift|proof_lift|load_or_build|load_or_build_with_identity|try_load_or_build|try_load_or_build_with_identity|model_identity_or_build|shape_union::(load_shapes|shape_files))[[:space:]]*\('
producer_cli='(\.arg\("(build|feedback|sync|check-sync|slice-spec-worker)"\)|\.args\(\[[^]]*"(build|feedback|sync|check-sync|slice-spec-worker)")'
real_docs_render='render_(site|site_lang|site_lang_exec|book)[[:space:]]*\('
test_side_refresh='(GMEOW_[A-Z0-9_]*BLESS|GMN1_VECTORS_BLESS|UPDATE_GOLDENS|REGEN_[A-Z0-9_]*|bless::write_expected)'

failures=()

# Print producer-call hits except a call explicitly identified as construction of a tiny,
# controlled synthetic input. The marker may sit on the call or one of the three adjacent
# lines so multiline Rust calls remain readable. It never authorizes repository paths or
# authenticated-product inputs; those are corpus production regardless of size.
scan_producer_calls() {
    awk '
        BEGIN {
            producer_re = ARGV[1]
            ARGV[1] = ""
            repository_bound_re = ARGV[2]
            ARGV[2] = ""
        }
        { source[NR] = $0 }
        END {
            for (line = 1; line <= NR; line++) {
                trimmed = source[line]
                sub(/^[[:space:]]*/, "", trimmed)
                if (source[line] !~ producer_re || substr(trimmed, 1, 2) == "//") {
                    continue
                }
                synthetic = 0
                for (near = line - 3; near <= line + 3; near++) {
                    if (near > 0 && source[near] ~ /gmeow-test-input: synthetic-only/) {
                        synthetic = 1
                    }
                }
                if (!synthetic || source[line] ~ repository_bound_re) {
                    print line ":" source[line]
                }
            }
        }
    ' "$producer_calls" "$repository_bound_calls"
}

mapfile -t integration_tests < <(
    rg -l -g 'tests.rs' -g '**/tests/*.rs' -g '**/tests/**/*.rs' \
        "${producer_calls}|${producer_cli}|${real_docs_render}|${test_side_refresh}" crates \
        | sort -u
)
for file in "${integration_tests[@]}"; do
    if ! scan_output=$(scan_producer_calls < "$file"); then
        failures+=("$file: corpus-producer scanner failed")
        continue
    fi
    while IFS= read -r hit; do
        [[ -z "$hit" ]] && continue
        failures+=("$file:$hit")
    done <<< "$scan_output"

    # A renderer over the authenticated whole-repository model is corpus production.
    # Small controlled models remain legitimate unit fixtures.
    while IFS=: read -r line_no source; do
        [[ -z "$line_no" ]] && continue
        start=$((line_no > 12 ? line_no - 12 : 1))
        if sed -n "${start},${line_no}p" "$file" | grep -Eq 'common::cached_model[[:space:]]*\('; then
            failures+=("$file:$line_no:$source")
        fi
    done < <(grep -En "$real_docs_render" "$file" | grep -Ev '^[0-9]+:[[:space:]]*//' || true)

    # Tests may inspect producer command text but may not execute a producer CLI.
    while IFS= read -r hit; do
        failures+=("$file:$hit")
    done < <(grep -En "$producer_cli" "$file" | grep -Ev '^[0-9]+:[[:space:]]*//' || true)

    while IFS= read -r hit; do
        failures+=("$file:$hit")
    done < <(grep -En "$test_side_refresh" "$file" | grep -Ev '^[0-9]+:[[:space:]]*//' || true)
done

# Inline unit tests and standalone test-only helpers live beside production code. Extract
# every complete cfg(test) item with comments/literals blanked before brace balancing, so a
# top-level helper cannot hide outside a conventional `mod tests` tail.
mapfile -t inline_test_files < <(
    rg -l "${producer_calls}|${test_side_refresh}" crates -g '*.rs' \
        | xargs -r grep -lE '#\[cfg\([^]]*test' \
        | sort -u
)
if ((${#inline_test_files[@]})); then
    while IFS= read -r hit; do
        [[ -z "$hit" ]] && continue
        failures+=("$hit")
    done < <(perl scripts/scan-cfg-test-producers.pl \
        "$producer_calls" "$repository_bound_calls" "$test_side_refresh" \
        "${inline_test_files[@]}")
fi

# Rustdoc examples are tests too. Inspect executable Rust fences in source docs and
# README files that can be included as crate docs; prose and explicitly non-Rust fences
# are ignored. A doctest may demonstrate a tiny synthetic transform, but it may never
# reach a repository-bound producer or a refresh/bless surface.
mapfile -t doctest_files < <(
    rg -l "${producer_calls}|${producer_cli}|${test_side_refresh}" crates \
        -g '*.rs' -g 'README.md' \
        | sort -u
)
if ((${#doctest_files[@]})); then
    while IFS= read -r hit; do
        [[ -z "$hit" ]] && continue
        failures+=("$hit")
    done < <(perl scripts/scan-doctest-producers.pl \
        "$producer_calls" "$repository_bound_calls" "$test_side_refresh" "$producer_cli" \
        "${doctest_files[@]}")
fi

# A build script is executed while test binaries are compiled. It therefore may not
# hide a corpus producer either; all current build scripts are input hashing/embedding
# only. Keep this scan explicit so a future build.rs cannot become test setup.
mapfile -t build_scripts < <(find crates -type f -name build.rs | sort)
for file in "${build_scripts[@]}"; do
    if ! scan_output=$(scan_producer_calls < "$file"); then
        failures+=("$file:build-script: corpus-producer scanner failed")
        continue
    fi
    while IFS= read -r hit; do
        [[ -z "$hit" ]] && continue
        failures+=("$file:build-script:$hit")
    done <<< "$scan_output"
done

make_recipe() {
    local target=$1
    awk -v target="$target" '
        $0 ~ "^" target ":" { active = 1; next }
        active && /^[^[:space:]#][^:]*:/ { exit }
        active { print }
    ' Makefile
}

for target in nextest nextest-archive maint-rust-heavy; do
    recipe=$(make_recipe "$target")
    if printf '%s\n' "$recipe" | grep -Eq '(test-fixtures[[:space:]]+produce|produce-test-fixtures|produce-producer-|prime-test-fixtures|prime-producer-)'; then
        failures+=("Makefile:$target invokes a corpus producer from a test-facing target")
    fi
    header=$(grep -En "^${target}:" Makefile | head -n 1)
    if [[ $header != *verify-test-fixtures* ]]; then
        failures+=("Makefile:$target lacks the fail-closed verify-test-fixtures prerequisite")
    fi
done

# No Make target that launches a test process may hide a repository-corpus producer in
# the same recipe. Explicit producer targets are separate DAG nodes; test recipes only
# authenticate and consume their results.
mapfile -t make_targets < <(grep -E '^[A-Za-z0-9_.-]+:.*## ' Makefile | cut -d: -f1)
for target in "${make_targets[@]}"; do
    recipe=$(make_recipe "$target")
    if ! printf '%s\n' "$recipe" | grep -Eq '(cargo (nextest run|test)|cargo llvm-cov nextest|node --test)'; then
        continue
    fi
    if printf '%s\n' "$recipe" | grep -Eq '(test-fixtures[[:space:]]+produce|produce-test-fixtures|produce-producer-|prime-test-fixtures|prime-producer-|check-sync[[:space:]].*SYNC_MODE=update|gmeow-dev[[:space:]]+sync)'; then
        failures+=("Makefile:$target combines a corpus producer with a test runner")
    fi
done

if ((${#failures[@]})); then
    printf 'ERROR: tests may never rebuild the corpus; producer-reachable test code is malicious:\n' >&2
    printf '  %s\n' "${failures[@]}" >&2
    exit 1
fi

echo "test corpus purity OK: tests consume authenticated corpus products only"
