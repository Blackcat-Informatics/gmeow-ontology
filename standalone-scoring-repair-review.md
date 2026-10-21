# Standalone scoring repair: independent source review

Scope: `report.rs::admit_projection_inputs`, its actual file/directory/public-standard callers, and the three `projection_literal_standalone_` controls in `profile.tests.rs`. Read the existing qualification record and installed-scoring snapshot. No builds, tests, formatting, corpus/producer operations or host probes were run. This is source review, not runtime acceptance or a whole-unit verdict.

The regular-file repair closes the observed omission. `score_slice_files_with_dataset` calls admission before axis evaluation; it admits the original semantic dataset plus separately parsed `shapes.ttl` and recursively named `mappings/**/*.ttl` documents. The independent `GroundingCatalog` remains a separate argument: neither the additional documents nor imported semantic facts augment catalog authority. `ScoreContext` still receives the original semantic dataset, so these added admission inputs and catalog premises do not enter an unrelated axis's graph denominator.

The real public `score_slice_files_with_standard` delegates to that function. Its directory twin reads the file map and delegates to the same public scorer. Bundle-facing file and directory operations likewise reach this common boundary. Thus the repair does not depend solely on the new test adapter.

The three controls cover an unresolved required alias with an actionable identity assertion; a valid nested mapping with unchanged per-axis scores and matching file/directory corpus identity; and missing-catalog/unknown-alias refusal through external and public-standard file/directory entry points. They use independently authored source and temporary regular files, without loading or producing the real corpus. The earlier installed-scoring snapshot missed the upstream file-selection omission; its broad no-bypass conclusion was superseded by the actual fourth-checkpoint counterexample recorded in `qualification.md`.

## Remaining directory-source omission

`slice_files_from_dir::walk` reads `DirEntry::file_type` and handles only `is_dir()` and `is_file()`. A symbolic-link entry falls through without an error. Consequently a symlinked `mappings/selected.ttl`, or a symlinked mapping directory, is omitted before `admit_projection_inputs` can inspect it. An explicit file-map caller supplying the same mapping bytes admits/refuses them normally. The ratchet's `collect_mapping_ttls` follows links and checks cycles, making this more than an assumed universal exclusion of links. The file-map loader's own whole-tree/parity contract does not state a symlink exclusion.

This behavior predates the helper but leaves a concrete caller-boundary gap in the repaired operation. Choose an explicit policy: refuse links with actionable source paths, or follow them under an appropriate cycle policy; silently ignoring them does not establish file/directory admission parity. The three new controls create regular files only and do not cover this case. Reported to root before qualification; no source repair or probe performed by this reviewer.

Root accepted the finding and selected explicit actionable refusal for every encountered symlink (file, directory or broken). The file-map contract carries regular-file bytes and has no link identity representation. The source owner will add the narrow refusal, documentation and independent Unix controls; review of that repair is pending. This reviewer made no production source changes.

Apart from that finding, no additional regular-file catalog-authority or semantic-denominator defect was found in the bounded inspected paths. Shape-file admission is present in source but is not specifically exercised by these three controls. The planned 27-control rerun, installed/embedded compilation and actual authenticated production acceptance remain outside this source review.

## Narrow final repair disposition

Reviewed the refrozen symlink delta in `report.rs` and the new Unix-only `projection_literal_directory_scoring_refuses_linked_selected_files_subtrees_and_dangling_links` control. The walker now rejects `file_type.is_symlink()` before either directory recursion or regular-file handling, using the offending entry's actual path. This covers links to files, directories and absent targets without following the target, so none can disappear silently from the admitted map. The documented input contract now explicitly requires regular files and describes link refusal.

The independent authored control constructs each of those three link kinds and exercises both actual directory entry points: bundle-standard external scoring and the public measurement-standard scorer. Each assertion requires both the symlink reason and the precise link path, preventing an unrelated admission failure from satisfying the test. The earlier positive regular-directory parity control remains unchanged. No source-level defect found in this narrow repair; the recorded omission is addressed in source.

The focused inventory is now 28 controls on Unix. This reviewer did not run them or compile the code; runtime success, installed/embedded compilation and actual authenticated production acceptance remain unverified by this review. No production source was edited.
