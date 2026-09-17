// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn deficiency_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_deficiency_emergency_ledger(root, &mut report);
    report.errors
}

#[test]
fn canonical_notice_only_deficiency_ledger_passes() {
    let temp = tempfile::tempdir().unwrap();
    write(&temp.path().join(".deficiencies"), EMPTY_DEFICIENCY_LEDGER);
    assert!(deficiency_errors(temp.path()).is_empty());
}

#[test]
fn any_deficiency_entry_fails_the_repository_gate() {
    let temp = tempfile::tempdir().unwrap();
    write(
        &temp.path().join(".deficiencies"),
        &format!(
            "{EMPTY_DEFICIENCY_LEDGER}\n## failed work\nThis must never pass as accepted risk.\n"
        ),
    );
    let errors = deficiency_errors(temp.path());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("unauthorized critically undone work"));
    assert!(errors[0].contains("blocks completion"));
}

#[test]
fn a_weakened_notice_or_moved_entry_fails_the_repository_gate() {
    let temp = tempfile::tempdir().unwrap();
    write(
        &temp.path().join(".deficiencies"),
        &EMPTY_DEFICIENCY_LEDGER.replace("100% UNAUTHORIZED", "sometimes unauthorized"),
    );
    let errors = deficiency_errors(temp.path());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("byte-for-byte"));
    assert!(errors[0].contains("cannot be moved above the marker"));
}

#[test]
fn a_missing_or_duplicate_deficiency_marker_fails_the_repository_gate() {
    let missing = tempfile::tempdir().unwrap();
    write(
        &missing.path().join(".deficiencies"),
        "# DEFICIENCY EMERGENCY LEDGER\n",
    );
    let missing_errors = deficiency_errors(missing.path());
    assert_eq!(missing_errors.len(), 1, "{missing_errors:?}");
    assert!(missing_errors[0].contains("found 0"));

    let duplicate = tempfile::tempdir().unwrap();
    write(
        &duplicate.path().join(".deficiencies"),
        &format!("{EMPTY_DEFICIENCY_LEDGER}{DEFICIENCY_ENTRY_MARKER}\n"),
    );
    let duplicate_errors = deficiency_errors(duplicate.path());
    assert_eq!(duplicate_errors.len(), 1, "{duplicate_errors:?}");
    assert!(duplicate_errors[0].contains("found 2"));
}

#[test]
fn real_checkout_without_deficiency_ledger_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    fs::create_dir_all(temp.path().join(".git")).unwrap();

    let report = check_repo_static(temp.path());
    assert!(
        report.errors.iter().any(|error| {
            error.contains(".deficiencies") && error.contains("missing or unreadable")
        }),
        "{:?}",
        report.errors
    );
}

/// Build a minimal tree carrying just the wasm-bindgen pin surface the parity guard
/// reads: the workspace pin, one `*-wasm` member, and the CI CLI install line.
fn write_wasm_bindgen_tree(root: &Path, workspace_pin: &str, member: &str, cli: &str) {
    write(
        &root.join("Cargo.toml"),
        &format!("[workspace.dependencies]\nwasm-bindgen = {workspace_pin}\n"),
    );
    write(
        &root.join("crates/query-wasm/Cargo.toml"),
        &format!("[dependencies]\n{member}\n"),
    );
    write(&root.join("Makefile"), "BINARYEN_VER := version_130\n");
    write(
        &root.join(".github/workflows/ci.yml"),
        &format!(
            "jobs:\n  heavy:\n    steps:\n      - with:\n          tool: wasm-bindgen-cli@{cli}\n      - run: make print-binaryen-ver\n"
        ),
    );
}

fn wasm_bindgen_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_wasm_bindgen_pin_parity(root, &mut report);
    report.errors
}

#[test]
fn wasm_bindgen_parity_accepts_one_workspace_pin_matching_the_ci_cli() {
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"=0.2.125\"",
        "wasm-bindgen.workspace = true",
        "0.2.125",
    );
    assert!(
        wasm_bindgen_errors(tmp.path()).is_empty(),
        "the agreeing arrangement must pass"
    );
}

#[test]
fn wasm_bindgen_parity_rejects_a_member_that_redeclares_the_version() {
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"=0.2.125\"",
        "wasm-bindgen = \"=0.2.100\"",
        "0.2.125",
    );
    let errors = wasm_bindgen_errors(tmp.path());
    assert!(
        errors.iter().any(|e| e.contains("redeclares wasm-bindgen")),
        "a member redeclaring the pin must red: {errors:?}"
    );
}

#[test]
fn wasm_bindgen_parity_rejects_a_ci_cli_that_disagrees_with_the_library_pin() {
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"=0.2.125\"",
        "wasm-bindgen.workspace = true",
        "0.2.100",
    );
    let errors = wasm_bindgen_errors(tmp.path());
    assert!(
        errors
            .iter()
            .any(|e| e.contains("installs wasm-bindgen-cli@0.2.100")),
        "a CLI disagreeing with the library pin must red: {errors:?}"
    );
}

#[test]
fn wasm_bindgen_parity_rejects_an_absent_ci_workflow() {
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"=0.2.125\"",
        "wasm-bindgen.workspace = true",
        "0.2.125",
    );
    fs::remove_file(tmp.path().join(".github/workflows/ci.yml")).unwrap();
    let errors = wasm_bindgen_errors(tmp.path());
    assert!(
        errors.iter().any(|e| e.contains("cannot read")),
        "an absent CI workflow must red — the CLI version cannot be checked, which is a \
             failure, not agreement: {errors:?}"
    );
}

#[test]
fn wasm_bindgen_parity_never_passes_silently_on_an_unparsable_root_manifest() {
    // The sharp case the old shape got wrong: `let Ok(text) = … else { return; }` made an
    // unreadable manifest a SILENT pass, so a tree with a real CLI drift reported nothing
    // at all. The comparison against the library pin genuinely cannot be performed
    // without the manifest — but the run must red saying so, never go quiet.
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"=0.2.125\"",
        "wasm-bindgen.workspace = true",
        "0.2.99",
    );
    write(
        &tmp.path().join("Cargo.toml"),
        "this is not valid toml = = =\n",
    );
    let errors = wasm_bindgen_errors(tmp.path());
    assert!(
        !errors.is_empty(),
        "an unparsable root manifest must not be a silent pass"
    );
    assert!(
        errors.iter().any(|e| e.contains("cannot parse")),
        "the failure must name the manifest it could not read: {errors:?}"
    );
}

#[test]
fn wasm_bindgen_parity_rejects_a_ci_binaryen_literal() {
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"=0.2.125\"",
        "wasm-bindgen.workspace = true",
        "0.2.125",
    );
    write(
        &tmp.path().join(".github/workflows/ci.yml"),
        "jobs:\n  heavy:\n    steps:\n      - with:\n          tool: wasm-bindgen-cli@0.2.125\n      - env:\n          BINARYEN_VER: version_119\n",
    );
    let errors = wasm_bindgen_errors(tmp.path());
    assert!(
        errors
            .iter()
            .any(|e| e.contains("binaryen release literal")),
        "a literal binaryen pin in CI must red — it is the two-constant drift that broke \
             the wasm lanes: {errors:?}"
    );
}

#[test]
fn wasm_bindgen_parity_rejects_a_shell_form_binaryen_literal() {
    // The idiom the repository actually uses is a shell assignment; a detector keyed
    // to the YAML-mapping form alone matches nothing here.
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"=0.2.125\"",
        "wasm-bindgen.workspace = true",
        "0.2.125",
    );
    write(
        &tmp.path().join(".github/workflows/ci.yml"),
        "jobs:\n  heavy:\n    steps:\n      - with:\n          tool: wasm-bindgen-cli@0.2.125\n      - run: make print-binaryen-ver\n      - run: BINARYEN_VER=\"version_119\" ./install.sh\n",
    );
    let errors = wasm_bindgen_errors(tmp.path());
    assert!(
        errors.iter().any(|e| e.contains("version_119")),
        "a shell-form binaryen literal must red: {errors:?}"
    );
}

#[test]
fn gts_chokepoint_rejects_a_direct_production_emit_gts_call() {
    let tmp = tempfile::tempdir().unwrap();
    // gmeow-test-input: synthetic-only
    write(
        &tmp.path().join("crates/rogue/src/lib.rs"),
        "pub fn ship(x: &purrdf::gts_compose::Snapshot) -> Vec<u8> {\n    purrdf::gts_compose::emit_gts(x)\n}\n",
    );
    let mut report = RepoStaticReport::default();
    check_gts_emit_chokepoint(tmp.path(), &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("calls the GTS writer directly")),
        "a production emit_gts call outside the profile crate must red: {:?}",
        report.errors
    );
}

#[test]
fn gts_chokepoint_permits_the_profile_crate_and_test_code() {
    let tmp = tempfile::tempdir().unwrap();
    // The permitted entry itself.
    // gmeow-test-input: synthetic-only
    write(
        &tmp.path().join("crates/gts-profile/src/lib.rs"),
        "pub fn emit(x: &purrdf::gts_compose::Snapshot) -> Vec<u8> {\n    purrdf::gts_compose::emit_gts(x)\n}\n",
    );
    // An integration-test crate driving the writer deliberately.
    // gmeow-test-input: synthetic-only
    write(
        &tmp.path().join("crates/consumer/tests/audit.rs"),
        "fn nonconforming() { let _ = purrdf::gts_compose::emit_gts(&x); }\n",
    );
    // A `#[cfg(test)]`-gated import inside production source.
    write(
        &tmp.path().join("crates/consumer/src/lib.rs"),
        "#[cfg(test)]\nuse purrdf::gts_compose::emit_gts;\n",
    );
    let mut report = RepoStaticReport::default();
    check_gts_emit_chokepoint(tmp.path(), &mut report);
    assert!(
        report.errors.is_empty(),
        "the profile crate and test code are the permitted callers: {:?}",
        report.errors
    );
}

#[test]
fn wasm_bindgen_parity_rejects_a_workspace_without_the_declaration() {
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"=0.2.125\"",
        "wasm-bindgen.workspace = true",
        "0.2.125",
    );
    write(
        &tmp.path().join("Cargo.toml"),
        "[workspace.dependencies]\nserde = \"1\"\n",
    );
    let errors = wasm_bindgen_errors(tmp.path());
    assert!(
        errors
            .iter()
            .any(|e| e.contains("declares no wasm-bindgen")),
        "a workspace without the declaration must red: {errors:?}"
    );
}

#[test]
fn wasm_bindgen_parity_rejects_a_range_workspace_pin() {
    let tmp = tempfile::tempdir().unwrap();
    write_wasm_bindgen_tree(
        tmp.path(),
        "\"0.2.125\"",
        "wasm-bindgen.workspace = true",
        "0.2.125",
    );
    let errors = wasm_bindgen_errors(tmp.path());
    assert!(
        errors.iter().any(|e| e.contains("a range")),
        "a caret pin must red — the CLI matches one version only: {errors:?}"
    );
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn write_minimal_repo(root: &Path) {
    // First-party code is rdflib-free (no keeper): a minimal valid repo has a
    // real src/gmeow_tools package with NO upstream-rdflib import anywhere (it
    // uses the purrdf.compat.rdflib facade instead).
    write(&root.join("src/gmeow_tools/sparql.py"), "import purrdf\n");
    // The wasm-bindgen parity guard treats the root manifest and the CI pin lines as
    // REQUIRED inputs, so the minimal repo carries an agreeing set of all three.
    write(
        &root.join("Cargo.toml"),
        "[workspace.dependencies]\nwasm-bindgen = \"=0.2.125\"\n",
    );
    write(
        &root.join(".github/workflows/ci.yml"),
        "on:\n  push:\n  pull_request:\njobs:\n  lint:\n    steps:\n      - run: make lint\n      - with:\n          tool: wasm-bindgen-cli@0.2.125\n      - run: make print-binaryen-ver\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
    );
    // The Docker-free reality: no target reaches Docker/Java. The ELK/HermiT
    // lane and its maint-reason-hermit / maint-verify-docker / maint-pull-images
    // targets are gone.
    write(
        &root.join("Makefile"),
        "check:\n\t$(MAKE) lint\nlint:\n\ttrue\n",
    );
    // `slices/` is a REQUIRED source tree (hand_authored_shapes_ttl_census hard-fails
    // when it is missing/unreadable) — an empty directory satisfies the requirement
    // without pinning any hand-authored shapes.ttl.
    fs::create_dir_all(root.join("slices")).unwrap();
    // `shapes/gmeow-shapes.ttl` is the drained root validation anchor: it MUST exist
    // (its consumers enumerate it) and declare zero hand-authored shapes.
    write(
        &root.join("shapes/gmeow-shapes.ttl"),
        "# Drained root validation anchor: every obligation lives in the logic: canon.\n",
    );
}

#[test]
fn projection_compute_purity_flags_unbacked_construct_and_passes_a_backed_one() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let module = root.join("slices/core/demo/module.ttl");

    // A hand-authored SHACL-AF derivation rule with NO logic:formalizes back-reference
    // is a forbidden second source of truth → the gate must fail.
    write(
        &module,
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
    );
    let mut report = RepoStaticReport::default();
    check_projection_compute_purity(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("computational SHACL-AF") && e.contains("module.ttl")),
        "a hand-authored sh:SPARQLRule without logic:formalizes must be flagged; got {:?}",
        report.errors
    );

    // The SAME construct WITH a logic:formalizes back-reference is the legal Hybrid
    // placement (it names its logic: source) → the gate must pass.
    write(
        &module,
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:S a sh:NodeShape ;\n    \
                 logic:formalizes ex:someLogicRule ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
    );
    let mut backed = RepoStaticReport::default();
    check_projection_compute_purity(root, &mut backed);
    assert!(
        backed.errors.is_empty(),
        "a logic:formalizes-backed construct must pass the purity gate; got {:?}",
        backed.errors
    );
}

#[test]
fn purity_gate_catches_alternate_prefix_and_full_iri_bypass() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // Alternate prefix bound to the SHACL namespace — a substring scan for "sh:rule" misses it.
    write(
        &root.join("slices/core/altprefix/module.ttl"),
        "@prefix af: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a af:NodeShape ;\n    \
                 af:rule [ a af:SPARQLRule ; \
                 af:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
    );
    // Full-IRI form — no SHACL prefix at all.
    write(
        &root.join("slices/core/fulliri/module.ttl"),
        "@prefix ex: <https://example.org/> .\n\
             ex:T a <http://www.w3.org/ns/shacl#NodeShape> ;\n    \
                 <http://www.w3.org/ns/shacl#rule> [ a <http://www.w3.org/ns/shacl#SPARQLRule> ; \
                 <http://www.w3.org/ns/shacl#construct> \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
    );
    let mut report = RepoStaticReport::default();
    check_projection_compute_purity(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("altprefix/module.ttl")),
        "an alternate-prefix computational construct must be flagged; got {:?}",
        report.errors
    );
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("fulliri/module.ttl")),
        "a full-IRI computational construct must be flagged; got {:?}",
        report.errors
    );
}

#[test]
fn purity_gate_rejects_backref_on_unrelated_node_or_in_a_comment() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // logic:formalizes is present but on an UNRELATED node, not on the construct's shape — a
    // file-scoped substring check would wrongly pass this.
    write(
        &root.join("slices/core/unrelated/module.ttl"),
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:Unrelated logic:formalizes ex:somewhere .\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
    );
    // logic:formalizes appears ONLY in a comment → no triple → must still be flagged.
    write(
        &root.join("slices/core/comment/module.ttl"),
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             # ex:S logic:formalizes ex:source -- a comment is not a triple\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
    );
    let mut report = RepoStaticReport::default();
    check_projection_compute_purity(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("unrelated/module.ttl")),
        "a back-reference on an unrelated node must NOT legalize the construct; got {:?}",
        report.errors
    );
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("comment/module.ttl")),
        "a back-reference present only in a comment must NOT legalize the construct; got {:?}",
        report.errors
    );
}

#[test]
fn collect_ttl_files_skips_symlinked_dirs_without_looping() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dir = root.join("slices/core/loop");
    fs::create_dir_all(&dir).unwrap();
    write(
        &dir.join("real.ttl"),
        "@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n",
    );
    // A symlink back to an ancestor would make a naive recursion loop forever.
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("slices"), dir.join("cycle")).unwrap();
    let mut report = RepoStaticReport::default();
    let mut out = Vec::new();
    // Must terminate (no stack overflow / infinite loop) and still find the real file.
    collect_ttl_files(&root.join("slices"), &mut report, &mut out);
    assert!(
        out.iter().any(|p| p.ends_with("real.ttl")),
        "the real .ttl must be collected; got {out:?}"
    );
}

#[test]
fn minimal_repo_passes() {
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    let report = check_repo_static(temp.path());
    assert!(report.ok(), "{:?}", report.errors);
}

/// Write one slice-owned Turtle surface with exactly the given bytes.
fn write_slice_ttl(root: &Path, rel: &str, body: &str) {
    write(&root.join("slices").join(rel), body);
}

#[test]
fn a_slice_ttl_without_a_trailing_newline_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write_slice_ttl(temp.path(), "core/demo/module.ttl", "ex:A a ex:B .");
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("core/demo/module.ttl") && e.contains("no trailing newline")),
        "{:?}",
        report.errors
    );
}

#[test]
fn the_gate_covers_every_slice_ttl_not_only_module_ttl() {
    // The bulk-edit failure mode reaches shapes.ttl, examples/, tests/ and
    // mappings/ too; a gate narrower than its fixer only moves the recurrence.
    for rel in [
        "core/demo/shapes.ttl",
        "core/demo/examples/sample.ttl",
        "core/demo/tests/structural.ttl",
        "core/demo/mappings/align.ttl",
    ] {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write_slice_ttl(temp.path(), rel, "ex:A a ex:B .");
        let report = check_repo_static(temp.path());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains(rel) && e.contains("no trailing newline")),
            "{rel} must be covered; got {:?}",
            report.errors
        );
    }
}

#[test]
fn a_slice_ttl_ending_in_a_blank_line_is_rejected() {
    // "Exactly one" is what the end-of-file hook normalizes to; accepting two
    // would let the gate and the fixer disagree about what correct looks like.
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write_slice_ttl(temp.path(), "core/demo/module.ttl", "ex:A a ex:B .\n\n");
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("core/demo/module.ttl") && e.contains("blank line")),
        "{:?}",
        report.errors
    );
}

#[test]
fn an_empty_slice_ttl_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write_slice_ttl(temp.path(), "core/demo/module.ttl", "");
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("core/demo/module.ttl") && e.contains("is empty")),
        "{:?}",
        report.errors
    );
}

#[test]
fn a_correctly_terminated_slice_ttl_passes() {
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write_slice_ttl(temp.path(), "core/demo/module.ttl", "ex:A a ex:B .\n");
    let report = check_repo_static(temp.path());
    assert!(report.ok(), "{:?}", report.errors);
}

#[test]
fn the_real_repository_holds_the_trailing_newline_invariant() {
    // The production surface, not a fixture. This is the assertion whose
    // absence let the 67-file state persist across several commits.
    let mut report = RepoStaticReport::default();
    check_slice_ttl_trailing_newline(live_repo_root(), &mut report);
    assert!(report.ok(), "{:?}", report.errors);
}

#[test]
fn required_ci_docker_token_fails() {
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write(
        &temp.path().join(".github/workflows/ci.yml"),
        "on:\n  pull_request:\njobs:\n  lint:\n    steps:\n      - run: docker run obolibrary/robot\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
    );
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("required CI job") && e.contains("docker"))
    );
}

#[test]
fn required_ci_job_container_token_fails() {
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write(
        &temp.path().join(".github/workflows/ci.yml"),
        "on:\n  pull_request:\njobs:\n  lint:\n    container: obolibrary/robot\n    steps:\n      - run: make lint\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
    );
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("required CI job") && e.contains("obolibrary/robot"))
    );
}

#[test]
fn makefile_target_reaching_docker_fails() {
    // No legitimate Docker lane exists: any target that reaches `docker` is a
    // re-introduction of the deleted ELK/HermiT lane and must be flagged.
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write(
        &temp.path().join("Makefile"),
        "check:\n\t$(MAKE) lint\nlint:\n\ttrue\nmaint-reason-hermit:\n\tdocker run obolibrary/robot\n",
    );
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("target \"maint-reason-hermit\"")
                && e.contains("reaches Docker/Java")),
        "{:?}",
        report.errors
    );
}

#[test]
fn makefile_target_reaching_java_fails() {
    // Java is banned everywhere too — the classic reasoner was a Java robot.jar.
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write(
        &temp.path().join("Makefile"),
        "check:\n\t$(MAKE) lint\nlint:\n\ttrue\nrobot:\n\tjava -jar robot.jar reason\n",
    );
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("target \"robot\"") && e.contains("reaches Docker/Java")),
        "{:?}",
        report.errors
    );
}

#[test]
fn makefile_target_invoking_pull_images_script_fails() {
    // pull-images.sh was deleted; shelling out to it re-introduces the lane.
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write(
        &temp.path().join("Makefile"),
        "check:\n\t$(MAKE) lint\nlint:\n\ttrue\npull:\n\tbash scripts/pull-images.sh\n",
    );
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("target \"pull\"") && e.contains("pull-images.sh")),
        "{:?}",
        report.errors
    );
}

#[test]
fn required_ci_oracle_reasoner_token_fails() {
    // The oracle-token ban ENFORCES the lane's removal: a required CI job that
    // invokes `--reasoner hermit` / `--reasoner elk` must still be rejected.
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write(
        &temp.path().join(".github/workflows/ci.yml"),
        "on:\n  pull_request:\njobs:\n  lint:\n    steps:\n      - run: gmeow-dev reason --reasoner hermit\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
    );
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("invokes the oracle lane") && e.contains("--reasoner hermit")),
        "{:?}",
        report.errors
    );
}

#[test]
fn differential_oracle_crosscheck_target_reintroduction_fails() {
    // The retired live native-vs-purrdf reason-crosscheck oracle. A
    // re-introduced `*-crosscheck` gate target regrows the forbidden lane;
    // the single-authority seal must red on it.
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write(
        &temp.path().join("Makefile"),
        "check:\n\t$(MAKE) lint\nlint:\n\ttrue\nfoo-crosscheck:\n\t$(GMEOW_DEV) foo-crosscheck\n",
    );
    let report = check_repo_static(temp.path());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("foo-crosscheck")
                && e.contains("live differential reasoning oracle")),
        "{:?}",
        report.errors
    );
}

#[test]
fn differential_oracle_seal_allows_retained_engine_independent_goldens() {
    // The retained native gap-zero DL-EL ledger is a committed
    // engine-independent golden referenced by ARTIFACT PATH in a recipe, and
    // the frozen oracle-gold is proven under `conformance` — neither is a
    // live-oracle GATE target, so the seal stays green.
    let temp = tempfile::tempdir().unwrap();
    write_minimal_repo(temp.path());
    write(
        &temp.path().join("Makefile"),
        "check:\n\t$(MAKE) reason-verify\n\
             reason-verify:\n\t$(GMEOW_DEV) reason-verify\n\
             conformance:\n\t$(GMEOW_DEV) conformance\n\
             release:\n\t--evidence generated/logic/dl-el-crosscheck-report.ttl\n",
    );
    let report = check_repo_static(temp.path());
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.contains("live differential reasoning oracle")),
        "retained engine-independent goldens must not trip the seal: {:?}",
        report.errors
    );
}

#[test]
fn shape_purity_flags_unbacked_migrated_axioms_and_passes_backed_and_closed_world() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let g = "https://blackcatinformatics.ca/gmeow/";

    // A hand-authored irreflexivity self-reference axiom with NO logic:formalizes → flagged.
    write(
        &root.join("shapes/bad-irreflexive.ttl"),
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:S a sh:NodeShape ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}counterGoal> $this . }}\"\"\" ] .\n"
        ),
    );
    // A hand-authored coincident-role distinctness axiom, unbacked → flagged.
    write(
        &root.join("shapes/bad-distinct.ttl"),
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:S a sh:NodeShape ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}committedAgent> ?v . $this <{g}commitmentBeneficiary> ?v . }}\"\"\" ] .\n"
        ),
    );
    // The SAME irreflexivity axiom WITH a logic:formalizes on its owning shape → legal.
    write(
        &root.join("shapes/good-backed.ttl"),
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
                 ex:S a sh:NodeShape ; logic:formalizes ex:someAxiom ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}counterGoal> $this . }}\"\"\" ] .\n"
        ),
    );
    // A retained closed-world check (FILTER NOT EXISTS existence) → NOT a migrated axiom,
    // must NOT be flagged even without logic:formalizes.
    write(
        &root.join("shapes/closed-world.ttl"),
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:S a sh:NodeShape ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}deonticModality> ?m . FILTER NOT EXISTS {{ $this <{g}normIssuer> ?i . }} }}\"\"\" ] .\n"
        ),
    );

    // The SAME irreflexivity axiom, unbacked, but padded with a newline + tab + extra
    // spaces between the predicate and `$this` — a whitespace re-encoding a single-space
    // `contains` check would miss. Must still be flagged.
    write(
        &root.join("shapes/bad-irreflexive-ws.ttl"),
        &format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:S a sh:NodeShape ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}counterGoal>\n\t  $this . }}\"\"\" ] .\n"
        ),
    );

    let mut report = RepoStaticReport::default();
    check_projection_shape_purity(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("bad-irreflexive.ttl")),
        "an unbacked irreflexivity self-reference axiom must be flagged; got {:?}",
        report.errors
    );
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("bad-irreflexive-ws.ttl")),
        "a whitespace-padded re-encoding of a migrated axiom must still be flagged; got {:?}",
        report.errors
    );
    assert!(
        report.errors.iter().any(|e| e.contains("bad-distinct.ttl")),
        "an unbacked coincident-role distinctness axiom must be flagged; got {:?}",
        report.errors
    );
    assert!(
        !report.errors.iter().any(|e| e.contains("good-backed.ttl")),
        "a logic:formalizes-backed axiom must pass; got {:?}",
        report.errors
    );
    assert!(
        !report.errors.iter().any(|e| e.contains("closed-world.ttl")),
        "a retained closed-world check must NOT be flagged; got {:?}",
        report.errors
    );
}

#[test]
fn live_repo_static_passes() {
    let report = check_repo_static(live_repo_root());
    assert!(report.ok(), "{:?}", report.errors);
}

/// The workspace root: `crates/validate` → `crates` → repo root.
fn live_repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("validate crate should live under crates/")
}

#[test]
fn declarative_gate_is_clean_on_the_migrated_tree() {
    // The BLANKET declarative-shape gate is now wired into `check_repo_static` (terminal
    // migration increment): the hand-authored shape corpus has been retired to the `logic:`
    // canon. Every authored `sh:NodeShape`/`sh:PropertyShape` remaining under the scanned
    // roots (slices/shapes/dsl/governance) is either deleted (grounded + re-projected) or a
    // backed boundary-kept residue carrying a `logic:formalizes` back-reference. So the gate
    // must be GREEN on the live tree — this is the terminal single-source-of-truth invariant.
    let mut report = RepoStaticReport::default();
    check_declarative_shape_purity(live_repo_root(), &mut report);
    assert!(
        report.errors.is_empty(),
        "declarative-shape purity gate must be clean on the migrated tree; an unbacked \
             authored shape remains (ground it in logic: and re-project, or add a \
             logic:formalizes back-reference to the boundary-kept residue): {:?}",
        report.errors
    );
}

#[test]
fn declarative_gate_flags_unbacked_node_shape_and_passes_backed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let module = root.join("slices/x/module.ttl");

    // An unbacked sh:NodeShape → flagged.
    write(
        &module,
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a sh:NodeShape ; sh:targetClass ex:Thing .\n",
    );
    let mut report = RepoStaticReport::default();
    check_declarative_shape_purity(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("module.ttl") && e.contains("declarative validation shape")),
        "an unbacked sh:NodeShape must be flagged; got {:?}",
        report.errors
    );

    // The SAME shape carrying logic:formalizes → legal.
    write(
        &module,
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:S a sh:NodeShape ; logic:formalizes ex:someConstraint ; sh:targetClass ex:Thing .\n",
    );
    let mut backed = RepoStaticReport::default();
    check_declarative_shape_purity(root, &mut backed);
    assert!(
        backed.errors.is_empty(),
        "a logic:formalizes-backed node shape must pass; got {:?}",
        backed.errors
    );
}

#[test]
fn declarative_gate_exempts_only_a_registered_fail_witness() {
    // Two hand-authored node shapes in the SAME `counter-examples/` directory. One is
    // registered as the slice's `gmeow:saFailWitness` for a `mustNot` structural assertion,
    // one is not. The registration is the whole difference: a witness that proves the ban
    // has teeth must not be punished by the scan, and a shape nobody registered must not
    // hide behind sharing its directory with one that was.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let ce = root.join("slices/z/tests/counter-examples");
    std::fs::create_dir_all(&ce).unwrap();
    write(
        &root.join("slices/z/manifest.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <https://example.org/slice> a gmeow:Slice .\n",
    );
    write(
        &root.join("slices/z/tests/structural.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <https://example.org/saNoShapes> a gmeow:StructuralAssertion ;\n\
             \x20\x20gmeow:saFailWitness \"tests/counter-examples/registered.ttl\" .\n",
    );
    let declarer = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a sh:NodeShape ; sh:targetClass ex:C .\n";
    write(&ce.join("registered.ttl"), declarer);
    write(&ce.join("smuggled.ttl"), declarer);

    let mut report = RepoStaticReport::default();
    check_declarative_shape_purity(root, &mut report);

    assert!(
        report.errors.iter().any(|e| e.contains("smuggled.ttl")),
        "an UNREGISTERED hand-authored shape in counter-examples/ must still be flagged — a \
             blanket directory exclusion is exactly the hole this closes: {:?}",
        report.errors
    );
    assert!(
        !report.errors.iter().any(|e| e.contains("registered.ttl")),
        "a REGISTERED gmeow:saFailWitness must stay exempt, or a mustNot structural \
             assertion could never carry non-vacuous evidence of its own ban: {:?}",
        report.errors
    );
}

#[test]
fn declarative_gate_walks_inline_property_shape_up_to_its_node_shape() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let module = root.join("slices/y/module.ttl");

    // An inline sh:property whose owning node shape is UNBACKED → both flagged.
    write(
        &module,
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:property [ sh:path ex:p ; sh:minCount 1 ] .\n",
    );
    let mut report = RepoStaticReport::default();
    check_declarative_shape_purity(root, &mut report);
    assert!(
        report.errors.iter().any(|e| e.contains("module.ttl")),
        "an inline property shape under an unbacked node shape must be flagged; got {:?}",
        report.errors
    );

    // With logic:formalizes on the OWNING node shape → the upward walk legalizes the inline
    // property shape too, so nothing is flagged.
    write(
        &module,
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:S a sh:NodeShape ; logic:formalizes ex:someConstraint ;\n    \
                 sh:property [ sh:path ex:p ; sh:minCount 1 ] .\n",
    );
    let mut backed = RepoStaticReport::default();
    check_declarative_shape_purity(root, &mut backed);
    assert!(
        backed.errors.is_empty(),
        "a backed node shape must legalize its inline property shape (upward walk); got {:?}",
        backed.errors
    );
}

#[test]
fn declarative_gate_catches_alternate_prefix_and_full_iri_bypass() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // Alternate prefix bound to the SHACL namespace — a substring scan for "sh:NodeShape"
    // misses it.
    write(
        &root.join("slices/altprefix/module.ttl"),
        "@prefix af: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a af:NodeShape ; af:targetClass ex:Thing .\n",
    );
    // Full-IRI form — no SHACL prefix at all.
    write(
        &root.join("slices/fulliri/module.ttl"),
        "@prefix ex: <https://example.org/> .\n\
             ex:T a <http://www.w3.org/ns/shacl#PropertyShape> ;\n    \
                 <http://www.w3.org/ns/shacl#path> ex:p .\n",
    );
    let mut report = RepoStaticReport::default();
    check_declarative_shape_purity(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("altprefix/module.ttl")),
        "an alternate-prefix declarative shape must be flagged; got {:?}",
        report.errors
    );
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("fulliri/module.ttl")),
        "a full-IRI declarative shape must be flagged; got {:?}",
        report.errors
    );
}

#[test]
fn authored_shex_gate_flags_a_shex_file_and_passes_when_absent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // No .shex present → passes.
    write(
        &root.join("shapes/gmeow-shapes.ttl"),
        "@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n",
    );
    let mut clean = RepoStaticReport::default();
    check_authored_shex_purity(root, &mut clean);
    assert!(
        clean.errors.is_empty(),
        "no authored .shex → gate passes; got {:?}",
        clean.errors
    );

    // A hand-authored .shex under shapes/ → flagged.
    write(&root.join("shapes/gmeow-shapes.shex"), "<S> { ex:p . }\n");
    let mut dirty = RepoStaticReport::default();
    check_authored_shex_purity(root, &mut dirty);
    assert!(
        dirty
            .errors
            .iter()
            .any(|e| e.contains("gmeow-shapes.shex") && e.contains("emit-only projection")),
        "an authored .shex surface must be flagged; got {:?}",
        dirty.errors
    );
}

// ── shrink-only shapes.ttl ratchet ───────────────────────────────────

#[test]
fn shapes_ratchet_hard_fails_when_slices_dir_is_missing() {
    // `slices/` is a REQUIRED source tree. The fail-open bug this guards against: an
    // absent/unreadable `slices/` used to make `hand_authored_shapes_ttl_census` return an
    // empty `Vec` silently, and an empty census is trivially a subset of
    // PINNED_HAND_AUTHORED_SHAPES_TTL, so the ratchet PASSED on a broken repo (.goals: "a
    // missing required thing is a HARD FAIL", never a silent pass).
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    // Deliberately no `slices/` dir at all.

    let mut report = RepoStaticReport::default();
    check_hand_authored_shapes_ratchet(root, &mut report);
    assert!(
        !report.ok(),
        "a missing required slices/ dir must hard-fail the gate, not pass silently"
    );
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("slices") && e.contains("cannot read required directory")),
        "{:?}",
        report.errors
    );
}

#[test]
fn shapes_ratchet_passes_when_census_is_empty_or_pinned() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // An empty (but present) slices/ dir → empty census, passes.
    fs::create_dir_all(root.join("slices")).unwrap();
    let mut none = RepoStaticReport::default();
    check_hand_authored_shapes_ratchet(root, &mut none);
    assert!(none.ok(), "{:?}", none.errors);

    // A shapes.ttl in a slice that IS on the pin → passes.
    write(&root.join("slices/core/ai/shapes.ttl"), "");
    let mut pinned = RepoStaticReport::default();
    check_hand_authored_shapes_ratchet(root, &mut pinned);
    assert!(pinned.ok(), "{:?}", pinned.errors);
}

#[test]
fn shapes_ratchet_flags_a_shapes_ttl_outside_the_pin() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    // "affect" is not in PINNED_HAND_AUTHORED_SHAPES_TTL.
    write(&root.join("slices/core/affect/shapes.ttl"), "");

    let mut report = RepoStaticReport::default();
    check_hand_authored_shapes_ratchet(root, &mut report);
    assert!(
        report.errors.iter().any(|e| {
            e.contains("slices/core/affect/shapes.ttl")
                && e.contains("MIGRATING-SHAPES-TO-LOGIC.md")
        }),
        "an unpinned shapes.ttl must be flagged and point at the migration doc; got {:?}",
        report.errors
    );
}

#[test]
fn gmeow_shapes_drained_passes_empty_and_flags_a_regrown_shape() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // A present, fully-drained file (comments only) → passes.
    write(
        &root.join("shapes/gmeow-shapes.ttl"),
        "# fully grounded in the logic: canon; shrink-only.\n",
    );
    let mut ok = RepoStaticReport::default();
    check_gmeow_shapes_drained(root, &mut ok);
    assert!(ok.ok(), "{:?}", ok.errors);

    // A re-introduced NodeShape → hard fail, pointing at the migration doc.
    write(
        &root.join("shapes/gmeow-shapes.ttl"),
        "gmeow:X a sh:NodeShape ; sh:targetClass gmeow:C .\n",
    );
    let mut bad = RepoStaticReport::default();
    check_gmeow_shapes_drained(root, &mut bad);
    assert!(
        bad.errors
            .iter()
            .any(|e| e.contains("shapes/gmeow-shapes.ttl")
                && e.contains("MIGRATING-SHAPES-TO-LOGIC.md")),
        "a regrown shape must be flagged; got {:?}",
        bad.errors
    );
}

#[test]
fn gmeow_shapes_drained_requires_the_file_to_exist() {
    let temp = tempfile::tempdir().unwrap();
    let mut report = RepoStaticReport::default();
    check_gmeow_shapes_drained(temp.path(), &mut report);
    assert!(
        report.errors.iter().any(|e| e.contains("must still exist")),
        "a missing drained file must be flagged; got {:?}",
        report.errors
    );
}

#[test]
fn shapes_ratchet_permits_shrinkage_without_touching_the_pin() {
    // Deleting a pinned slice's shapes.ttl (a retirement, e.g. slices/grounding/math's) must
    // never fail the gate even though the pin itself was not trimmed — subset-or-equal, not
    // strict equality.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("slices/grounding")).unwrap();
    let mut report = RepoStaticReport::default();
    check_hand_authored_shapes_ratchet(root, &mut report);
    assert!(report.ok(), "{:?}", report.errors);
}

#[test]
fn gts_slice_ships_no_hand_authored_shapes_ttl() {
    // The GTS transport slice is fully migrated: its four NodeShapes are now OWL restriction
    // axioms + two logic:Constraints in slices/core/gts/module.ttl, and the equivalence is
    // certified by crates/logic-compile/tests/shape_migration_equivalence.rs. The file must
    // be GONE (not emptied) and its entry trimmed from the shrink-only pin — a re-appearance
    // would be a second source of validation truth, and a lingering pin entry would silently
    // re-license one.
    const RETIRED: &str = "slices/core/gts/shapes.ttl";
    assert!(
        !live_repo_root().join(RETIRED).exists(),
        "{RETIRED} is retired — its obligations live in slices/core/gts/module.ttl \
             (docs/MIGRATING-SHAPES-TO-LOGIC.md); re-introducing it is a second source of truth"
    );
    assert!(
        !PINNED_HAND_AUTHORED_SHAPES_TTL.contains(&RETIRED),
        "{RETIRED} is retired and must not remain in PINNED_HAND_AUTHORED_SHAPES_TTL"
    );
}

#[test]
fn live_hand_authored_shapes_ttl_census_is_subset_or_equal_of_the_pin() {
    // Direct exercise of the invariant described on PINNED_HAND_AUTHORED_SHAPES_TTL: the live
    // repo's census may shrink relative to the pin (retirements land without a pin edit) but
    // must never grow beyond it.
    let pinned: BTreeSet<&str> = PINNED_HAND_AUTHORED_SHAPES_TTL.iter().copied().collect();
    let mut report = RepoStaticReport::default();
    let live = hand_authored_shapes_ttl_census(live_repo_root(), &mut report);
    assert!(
        report.ok(),
        "the live repo's slices/ tree must be readable: {:?}",
        report.errors
    );
    for rel in &live {
        assert!(
            pinned.contains(rel.as_str()),
            "{rel}: a hand-authored shapes.ttl exists outside the pinned shrink-only \
                 census — see docs/MIGRATING-SHAPES-TO-LOGIC.md before adding a new one",
        );
    }
}

// ── generated/-read ban ─────────────────────────────────────────────

#[test]
fn blank_pass_keeps_strings_blanks_comments_and_cfg_test_bodies() {
    let src = "let a = root.join(\"generated/x.rq\"); // prose generated/y\n\
                   #[cfg(test)]\n\
                   mod t {\n    fn f() { let _ = root.join(\"generated/z.rq\"); }\n}\n";
    let out = blank_comments_and_cfg_test_modules(src);
    // Real string literals survive (so the scanner can still see them)…
    assert!(out.contains(".join(\"generated/x.rq\""));
    // …the line comment is blanked (prose mentioning a generated/ path cannot match)…
    assert!(!out.contains("generated/y"));
    // …and the whole `#[cfg(test)] mod` body is blanked (test fixtures are exempt).
    assert!(!out.contains("generated/z"));
    // Line count is preserved so line numbers stay aligned.
    assert_eq!(out.lines().count(), src.lines().count());
}

fn stage_file(root: &Path, name: &str, body: &str) {
    write(
        &root.join(format!("crates/pipeline/src/stages/{name}")),
        body,
    );
}

fn ban_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_no_generated_read_in_pipeline_stages(root, &mut report);
    report.errors
}

#[test]
fn ban_flags_a_literal_generated_disk_read_in_a_produce_stage() {
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "fn run(root: &std::path::Path) {\n    let _ = list_files(&root.join(\"generated/queries\"), \"rq\");\n}\n",
    );
    let errs = ban_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("stale-disk-fold"), "{errs:?}");
}

#[test]
fn ban_flags_a_const_indirected_generated_disk_read() {
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "const DCAT: &str = \"generated/queries/dcat.rq\";\n\
             fn run(root: &std::path::Path) {\n    let _ = std::fs::read(root.join(DCAT));\n}\n",
    );
    let errs = ban_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
}

#[test]
fn ban_flags_a_borrowed_or_method_const_generated_read() {
    // Idiomatic indirections must not slip the ban: `.join(&NAME)` (borrow) and
    // `.join(NAME.as_str())` (method) both build a disk path from a generated const.
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "const DCAT: &str = \"generated/queries/dcat.rq\";\n\
             fn run(root: &std::path::Path) {\n\
             \x20   let _ = std::fs::read(root.join(&DCAT));\n\
             \x20   let _ = std::fs::read(root.join(DCAT.as_str()));\n}\n",
    );
    let errs = ban_errors(temp.path());
    assert_eq!(errs.len(), 2, "{errs:?}");
}

#[test]
fn ban_ignores_a_product_read_of_a_generated_const() {
    // Reading the artifact off a stage PRODUCT (`.artifact(NAME)`) is not a disk read.
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "const DCAT: &str = \"generated/queries/dcat.rq\";\n\
             fn run(p: &Product) {\n    let _ = p.artifact(DCAT);\n}\n",
    );
    assert!(ban_errors(temp.path()).is_empty());
}

#[test]
fn ban_exempts_cfg_test_modules() {
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "fn run() {}\n\
             #[cfg(test)]\nmod tests {\n    fn t(root: &std::path::Path) {\n        let _ = list_files(&root.join(\"generated/mappings\"), \"tsv\");\n    }\n}\n",
    );
    assert!(ban_errors(temp.path()).is_empty());
}

#[test]
fn ban_exempts_a_marked_audit_read() {
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "fn audit(root: &std::path::Path) {\n\
             \x20   // GENERATED-READ-OK: audit lane, lints committed output, never folds into gmeow.gts.\n\
             \x20   let _ = root.join(\"generated/mappings\");\n}\n",
    );
    assert!(ban_errors(temp.path()).is_empty());
}

#[test]
fn ban_ignores_prose_mentioning_generated_paths() {
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "fn run() {\n    // this fold used to read generated/queries off disk; now product-sourced.\n    let _ = 1;\n}\n",
    );
    assert!(ban_errors(temp.path()).is_empty());
}

// ── bypass-coverage: hardened literal regex + widened scan scope ─────

fn pipeline_src_file(root: &Path, name: &str, body: &str) {
    write(&root.join(format!("crates/pipeline/src/{name}")), body);
}

#[test]
fn ban_flags_a_whitespace_join_generated_read() {
    // A space after `.join(` — a naive `.contains(".join(\"generated")` misses it.
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "fn run(root: &std::path::Path) {\n    let _ = list_files(&root.join( \"generated/queries\"), \"rq\");\n}\n",
    );
    let errs = ban_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("stale-disk-fold"), "{errs:?}");
}

#[test]
fn ban_flags_a_format_join_generated_read() {
    // A `.join(format!("generated/{p}.rq"))` wrapper — must still be flagged.
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "fn run(root: &std::path::Path, p: &str) {\n    let _ = std::fs::read(root.join(format!(\"generated/{p}.rq\")));\n}\n",
    );
    let errs = ban_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("stale-disk-fold"), "{errs:?}");
}

#[test]
fn ban_flags_a_slashless_const_generated_read() {
    // A slash-less `const … = "generated";` used via `.join(G)` — the relaxed const regex
    // must catch the bare directory name, not only `"generated/…"`.
    let temp = tempfile::tempdir().unwrap();
    stage_file(
        temp.path(),
        "foo.rs",
        "const G: &str = \"generated\";\n\
             fn run(root: &std::path::Path) {\n    let _ = std::fs::read(root.join(G));\n}\n",
    );
    let errs = ban_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
}

#[test]
fn ban_flags_a_produce_read_outside_stages_dir() {
    // A produce read in a helper OUTSIDE stages/ — the old stages/-only scan would have
    // missed it; the widened recursive scan of crates/pipeline/src/ catches it.
    let temp = tempfile::tempdir().unwrap();
    pipeline_src_file(
        temp.path(),
        "helper.rs",
        "fn run(root: &std::path::Path) {\n    let _ = list_files(&root.join(\"generated/queries\"), \"rq\");\n}\n",
    );
    let errs = ban_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("stale-disk-fold"), "{errs:?}");
}

// ── honest-invariant #1: no first-party thiserror/anyhow dependency ──

fn crate_manifest(root: &Path, crate_name: &str, extra: &str) {
    write(
        &root.join(format!("crates/{crate_name}/Cargo.toml")),
        &format!(
            "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n{extra}\n"
        ),
    );
    write(
        &root.join(format!("crates/{crate_name}/src/lib.rs")),
        "// empty\n",
    );
}

#[test]
fn minimal_repo_with_a_clean_crate_passes_error_crate_dep_check() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_minimal_repo(root);
    crate_manifest(
        root,
        "gmeow-foo",
        "[dependencies]\nserde = \"1\"\n\n[dev-dependencies]\ntempfile = \"3\"\n",
    );
    let mut report = RepoStaticReport::default();
    check_no_first_party_error_crate_deps(root, &mut report);
    assert!(report.ok(), "{:?}", report.errors);
}

#[test]
fn thiserror_dependency_string_form_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_manifest(root, "gmeow-foo", "[dependencies]\nthiserror = \"1\"\n");
    let mut report = RepoStaticReport::default();
    check_no_first_party_error_crate_deps(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("gmeow-foo") && e.contains("thiserror")),
        "{:?}",
        report.errors
    );
}

#[test]
fn anyhow_workspace_dependency_form_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_manifest(
        root,
        "gmeow-bar",
        "[dev-dependencies]\nanyhow = { workspace = true }\n",
    );
    let mut report = RepoStaticReport::default();
    check_no_first_party_error_crate_deps(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("gmeow-bar") && e.contains("anyhow")),
        "{:?}",
        report.errors
    );
}

#[test]
fn thiserror_in_target_cfg_dependencies_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_manifest(
        root,
        "gmeow-baz",
        "[target.'cfg(not(target_arch = \"wasm32\"))'.dependencies]\nthiserror = \"1\"\n",
    );
    let mut report = RepoStaticReport::default();
    check_no_first_party_error_crate_deps(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("gmeow-baz") && e.contains("thiserror")),
        "{:?}",
        report.errors
    );
}

// ── delegation-purity: purrdf is the sole RDF/SHACL stack ────────────

#[test]
fn rdf_stack_ban_flags_a_competing_rdf_dep() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_manifest(root, "gmeow-foo", "[dependencies]\noxrdf = \"0.2\"\n");
    let mut report = RepoStaticReport::default();
    check_rdf_stack_is_purrdf_only(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("gmeow-foo") && e.contains("oxrdf")),
        "{:?}",
        report.errors
    );
}

#[test]
fn rdf_stack_ban_flags_a_workspace_form_shacl_dep() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_manifest(
        root,
        "gmeow-bar",
        "[dev-dependencies]\nsophia = { workspace = true }\n",
    );
    let mut report = RepoStaticReport::default();
    check_rdf_stack_is_purrdf_only(root, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("gmeow-bar") && e.contains("sophia")),
        "{:?}",
        report.errors
    );
}

#[test]
fn rdf_stack_ban_allows_the_purrdf_umbrella() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_manifest(
        root,
        "gmeow-foo",
        "[dependencies]\npurrdf = { workspace = true }\nserde = \"1\"\n",
    );
    let mut report = RepoStaticReport::default();
    check_rdf_stack_is_purrdf_only(root, &mut report);
    assert!(report.ok(), "{:?}", report.errors);
}

// ── purrdf-source parity + structured-zstd floor ─────────────────────

fn pin_gate_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_purrdf_and_zstd_pins(root, &mut report);
    report.errors
}

const PURRDF_RELEASE: &str = r#"purrdf = "2""#;

fn write_root_and_fuzz_purrdf(root: &Path, root_dep: &str, fuzz_dep: &str) {
    write(
        &root.join("Cargo.toml"),
        &format!(
            "[workspace.dependencies]\n{root_dep}\n{}\n",
            root_dep.replacen("purrdf =", "purrdf-core =", 1)
        ),
    );
    write(
        &root.join("fuzz/Cargo.toml"),
        &format!(
            "[package]\nname = \"gmeow-fuzz\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\n{fuzz_dep}\n"
        ),
    );
}

fn write_lock_with_structured_zstd(root: &Path, version: &str) {
    let mut lock = format!(
        "version = 4\n\n[[package]]\nname = 'structured-zstd'\nversion = '{version}'\nsource = 'registry+https://github.com/rust-lang/crates.io-index'\n"
    );
    for name in ["purrdf", "purrdf-core"] {
        lock.push_str(&format!(
                "\n[[package]]\nname = '{name}'\nversion = '2.0.0'\nsource = 'registry+https://github.com/rust-lang/crates.io-index'\nchecksum = '{}'\n",
                "a".repeat(64),
            ));
    }
    write(&root.join("Cargo.lock"), &lock);
    write(&root.join("fuzz/Cargo.lock"), &lock);
}

#[test]
fn pin_gate_passes_matching_source_and_zstd_floor() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_root_and_fuzz_purrdf(root, PURRDF_RELEASE, PURRDF_RELEASE);
    write_lock_with_structured_zstd(root, "0.0.49");
    assert!(
        pin_gate_errors(root).is_empty(),
        "{:?}",
        pin_gate_errors(root)
    );
}

#[test]
fn pin_gate_rejects_same_version_checksum_drift_and_asset_key_tracks_release() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_root_and_fuzz_purrdf(root, PURRDF_RELEASE, PURRDF_RELEASE);
    write_lock_with_structured_zstd(root, "0.0.49");
    let lock_path = root.join("Cargo.lock");
    let lock = format!(
        "{}\n[[package]]\nname = 'wasm-bindgen'\nversion = '0.2.125'\n",
        fs::read_to_string(&lock_path).unwrap()
    );
    write(&lock_path, &lock);
    write(&root.join("Makefile"), "BINARYEN_VER := version_130\n");
    let original = workspace_substrate_key(root).unwrap();
    assert!(original.contains("purrdf 2.0.0;"), "{original}");
    write(&lock_path, &lock.replace(&"a".repeat(64), &"b".repeat(64)));
    let errors = pin_gate_errors(root);
    assert!(
        errors
            .iter()
            .any(|err| err.contains("differs from Cargo.lock")),
        "{errors:?}"
    );
    let upgraded = lock.replace("2.0.0", "2.0.1");
    write(&lock_path, &upgraded);
    write(&root.join("fuzz/Cargo.lock"), &upgraded);
    assert_ne!(original, workspace_substrate_key(root).unwrap());
    assert!(pin_gate_errors(root).is_empty());
}

#[test]
fn pin_gate_flags_fuzz_purrdf_source_drift() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    // A different compatible-release family is not the production requirement.
    write_root_and_fuzz_purrdf(root, PURRDF_RELEASE, "purrdf = \"0.7\"");
    let errs = pin_gate_errors(root);
    assert!(
        errs.iter()
            .any(|e| e.contains("fuzz/Cargo.toml") && e.contains("purrdf")),
        "{errs:?}"
    );
}

#[test]
fn pin_gate_flags_fuzz_purrdf_tag_drift() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let fuzz_old =
        "purrdf = { git = \"https://example.invalid/purrdf.git\", tag = \"rust-v0.7.0\" }";
    write_root_and_fuzz_purrdf(root, PURRDF_RELEASE, fuzz_old);
    let errs = pin_gate_errors(root);
    assert!(
        errs.iter().any(|e| e.contains("fuzz/Cargo.toml")),
        "{errs:?}"
    );
}

#[test]
fn pin_gate_flags_structured_zstd_below_floor() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_root_and_fuzz_purrdf(root, PURRDF_RELEASE, PURRDF_RELEASE);
    write_lock_with_structured_zstd(root, "0.0.40");
    let errs = pin_gate_errors(root);
    assert!(
        errs.iter()
            .any(|e| e.contains("structured-zstd") && e.contains("0.0.49")),
        "{errs:?}"
    );
}

#[test]
fn pin_gate_skips_absent_inputs() {
    let temp = tempfile::tempdir().unwrap();
    assert!(pin_gate_errors(temp.path()).is_empty());
}

// ── honest-invariant #2: String is never a Result error type ─────────

fn crate_src(root: &Path, crate_name: &str, file: &str, body: &str) {
    write(&root.join(format!("crates/{crate_name}/src/{file}")), body);
}

fn string_result_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_no_string_result_error_type(root, &mut report);
    report.errors
}

#[test]
fn result_unit_string_return_type_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "fn f() -> Result<(), String> {\n    Ok(())\n}\n",
    );
    let errs = string_result_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("Result<_, String>"), "{errs:?}");
}

#[test]
fn result_u8_string_return_type_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "fn g() -> Result<u8, String> {\n    Ok(0)\n}\n",
    );
    let errs = string_result_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
}

#[test]
fn std_result_fully_qualified_string_error_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "fn h() -> std::result::Result<u8, String> {\n    Ok(0)\n}\n",
    );
    let errs = string_result_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
}

#[test]
fn ok_position_and_single_arg_and_nested_string_do_not_false_positive() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "use std::collections::BTreeMap;\n\
             fn a() -> Result<String> { Ok(String::new()) }\n\
             fn b() -> Result<BTreeMap<String, String>, Diag> { Ok(BTreeMap::new()) }\n\
             fn c() -> io::Result<String> { Ok(String::new()) }\n\
             fn d() -> Result<T, MyErr<String>> { unimplemented!() }\n",
    );
    let errs = string_result_errors(root);
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn string_result_in_comment_and_cfg_test_module_is_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "// fn old() -> Result<(), String> { unimplemented!() }\n\
             fn real() -> Result<(), Diag> { Ok(()) }\n\
             #[cfg(test)]\nmod tests {\n    fn t() -> Result<(), String> { Ok(()) }\n}\n",
    );
    let errs = string_result_errors(root);
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn parse_top_level_generic_args_splits_only_top_level_commas() {
    let text: Vec<char> = "<BTreeMap<String, String>, Diag>".chars().collect();
    let (args, end) = parse_top_level_generic_args(&text, 0).expect("balanced");
    assert_eq!(args, vec!["BTreeMap<String, String>", "Diag"]);
    assert_eq!(end, text.len());
}

// ── GTS-authorship seals (A: owning snapshot publication; B: no bypass) ────

/// A synthetic owning profile with one private writer and native receipt admission.
fn write_gts_profile_crate(root: &Path) {
    crate_src(
        root,
        "gts-profile",
        "lib.rs",
        // gmeow-test-input: synthetic-only
        "pub fn emit_gmeow_gts(builder: SnapshotBuilder, medium: MediumPlan) -> Result<GmeowGtsEmission> {\n\
             if builder.poison().is_some() { return Err(refusal()); }\n\
             let ingestion = builder.ingest_totals(); let payload = builder.snapshot_payload();\n\
             let bytes = emit_snapshot_payload_with_medium(payload, medium)?;\n\
             Ok(GmeowGtsEmission { bytes, ingestion, source_receipts: Vec::new() })\n}\n\
             fn emit_snapshot_payload_with_medium(payload: Value, medium: MediumPlan) -> Result<Vec<u8>> {\n\
             let writer = purrdf::gts::writer::Writer::with_options(\"dist\", options); finish(writer, payload)\n}\n",
    );
}

fn gts_hits(root: &Path) -> Vec<GtsAuthorshipHit> {
    let mut report = RepoStaticReport::default();
    let hits = purrdf_gts_authorship_census(root, &mut report);
    assert!(report.ok(), "census must not error: {:?}", report.errors);
    hits
}

fn gts_seal_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_gts_authorship_seals(root, &mut report);
    report.errors
}

// ── Seal C: the producer→medium map is TOTAL over production producers ────

/// A synthetic repo with six producer files (the non-vacuity floor) and a
/// `slices/` tree whose producer map is `declared` — spliced in verbatim so a
/// test can drop a row, duplicate a medium, or point at a medium with no source
/// kind, and watch exactly one clause fire.
fn producer_seal_repo(declared: &str, files: &[&str]) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    for (i, file) in files.iter().enumerate() {
        crate_src(
            root,
            &format!("gmeow-p{i}"),
            file,
            // gmeow-test-input: synthetic-only
            "fn go() { let _ = emit_gmeow_gts(b, v, v, None, &medium); }\n",
        );
    }
    let slices = root.join("slices/core/gts");
    fs::create_dir_all(&slices).unwrap();
    fs::write(
        slices.join("module.ttl"),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                 gmeow:mediumFixture gmeow:mediumSourceKind gmeow:mediumSourceWholeArtifact .\n\
                 gmeow:mediumNoKind a gmeow:Medium .\n\
                 {declared}"
        ),
    )
    .unwrap();
    temp
}

/// The six synthetic producer files, and the repo-relative paths they land at.
const PRODUCER_FIXTURE_FILES: [&str; 6] = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs"];

fn producer_fixture_paths() -> Vec<String> {
    PRODUCER_FIXTURE_FILES
        .iter()
        .enumerate()
        .map(|(i, f)| format!("crates/gmeow-p{i}/src/{f}"))
        .collect()
}

fn producer_seal_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_every_gts_producer_declares_a_medium(root, &mut report);
    report.errors
}

#[test]
fn seal_c_passes_when_every_producer_is_declared() {
    let rows: String = producer_fixture_paths()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "gmeow:prod{i} a gmeow:GtsProducer ; gmeow:producerCallSite {p:?} ; \
                     gmeow:producerMedium gmeow:mediumFixture .\n"
            )
        })
        .collect();
    let temp = producer_seal_repo(&rows, &PRODUCER_FIXTURE_FILES);
    let errs = producer_seal_errors(temp.path());
    assert!(errs.is_empty(), "{errs:?}");
}

/// The clause the whole split rests on: a production producer the ontology does
/// not classify has NO audit branch, so it would be audited by nothing.
#[test]
fn seal_c_fails_on_a_producer_with_no_declared_medium_source_kind() {
    let paths = producer_fixture_paths();
    // Five rows; the sixth producer is left unclassified.
    let rows: String = paths
        .iter()
        .take(5)
        .enumerate()
        .map(|(i, p)| {
            format!(
                "gmeow:prod{i} a gmeow:GtsProducer ; gmeow:producerCallSite {p:?} ; \
                     gmeow:producerMedium gmeow:mediumFixture .\n"
            )
        })
        .collect();
    let temp = producer_seal_repo(&rows, &PRODUCER_FIXTURE_FILES);
    let errs = producer_seal_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains(&paths[5]), "{errs:?}");
    assert!(
        errs[0].contains("no gmeow:GtsProducer declares it"),
        "{errs:?}"
    );
}

/// A row whose medium declares NO `gmeow:mediumSourceKind` is equally unbranched
/// — being listed is not the same as being classified.
#[test]
fn seal_c_fails_when_a_declared_medium_carries_no_source_kind() {
    let paths = producer_fixture_paths();
    let mut rows: String = paths
        .iter()
        .take(5)
        .enumerate()
        .map(|(i, p)| {
            format!(
                "gmeow:prod{i} a gmeow:GtsProducer ; gmeow:producerCallSite {p:?} ; \
                     gmeow:producerMedium gmeow:mediumFixture .\n"
            )
        })
        .collect();
    rows.push_str(&format!(
        "gmeow:prod5 a gmeow:GtsProducer ; gmeow:producerCallSite {:?} ; \
             gmeow:producerMedium gmeow:mediumNoKind .\n",
        paths[5]
    ));
    let temp = producer_seal_repo(&rows, &PRODUCER_FIXTURE_FILES);
    let errs = producer_seal_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        errs[0].contains("gmeow:mediumSourceKind value(s)"),
        "{errs:?}"
    );
}

/// A row claiming a file that authors nothing is STALE — the shape a real
/// producer's classifying row takes after its file is renamed out from under it.
#[test]
fn seal_c_fails_on_a_stale_call_site() {
    let mut rows: String = producer_fixture_paths()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "gmeow:prod{i} a gmeow:GtsProducer ; gmeow:producerCallSite {p:?} ; \
                     gmeow:producerMedium gmeow:mediumFixture .\n"
            )
        })
        .collect();
    rows.push_str(
        "gmeow:prodStale a gmeow:GtsProducer ; \
             gmeow:producerCallSite \"crates/gone/src/lib.rs\" ; \
             gmeow:producerMedium gmeow:mediumFixture .\n",
    );
    let temp = producer_seal_repo(&rows, &PRODUCER_FIXTURE_FILES);
    let errs = producer_seal_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("names no production file"), "{errs:?}");
}

/// A truncated census is a SUBSET of any pin, so it must fail loudly rather than
/// pass while proving nothing.
#[test]
fn seal_c_fails_when_the_census_is_below_the_non_vacuity_floor() {
    let temp = producer_seal_repo(
        "gmeow:prod0 a gmeow:GtsProducer ; \
             gmeow:producerCallSite \"crates/gmeow-p0/src/a.rs\" ; \
             gmeow:producerMedium gmeow:mediumFixture .\n",
        &["a.rs"],
    );
    let errs = producer_seal_errors(temp.path());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("non-vacuity floor"), "{errs:?}");
}

/// Seal C on the LIVE tree, positively: the census really does find the known
/// production producers, and every one of them resolves to exactly one declared
/// `gmeow:mediumSourceKind` — so a later "0 unclassified" result cannot be a
/// silent miss. The three source kinds are all exercised, which is what makes the
/// split a genuine partition rather than one live branch and two decorative ones.
#[test]
fn live_repo_producer_map_is_total_and_exercises_all_three_source_kinds() {
    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let mut report = RepoStaticReport::default();
    let census = gts_producer_census(root, &mut report);
    assert!(report.ok(), "census must not error: {:?}", report.errors);
    let files: BTreeSet<&str> = census.iter().map(|p| p.file.as_str()).collect();
    for known in [
        "crates/gmeow-dev-cli/src/feedback_bundle.rs",
        "crates/math/src/lib.rs",
        "crates/music/src/lib.rs",
        // The MCP engine is its own leaf crate now, and its runtime store opens its own
        // segment, so the census must find BOTH of its authorship doors.
        "crates/mcp/src/lib.rs",
        "crates/mcp/src/storage.rs",
        "crates/pipeline/src/stages/carrier.rs",
        "crates/transcode/src/lib.rs",
    ] {
        assert!(
            files.contains(known),
            "the live census must discover {known}; found {files:?}"
        );
    }

    let declared = declared_gts_producers(root, &mut report);
    assert!(report.ok(), "{:?}", report.errors);
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for file in &files {
        let entry = declared
            .get(*file)
            .unwrap_or_else(|| panic!("{file} carries no gmeow:GtsProducer row"));
        assert_eq!(entry.media.len(), 1, "{file}: {:?}", entry.media);
        assert_eq!(
            entry.source_kinds.len(),
            1,
            "{file}: {:?}",
            entry.source_kinds
        );
        kinds.extend(entry.source_kinds.iter().cloned());
    }
    assert_eq!(
        kinds,
        [
            format!("{GMEOW}mediumSourceHeaderDict"),
            format!("{GMEOW}mediumSourcePerRep"),
            format!("{GMEOW}mediumSourceWholeArtifact"),
        ]
        .into_iter()
        .collect::<BTreeSet<String>>(),
        "all three declared source kinds must be live, or a branch is decorative"
    );
    assert!(
        PINNED_GTS_PRODUCERS_WITHOUT_DECLARED_MEDIUM.is_empty(),
        "the shrink-only producer census must stay empty — the medium audit's split is a \
             total function over producers, so no producer may go unclassified"
    );
}

/// Non-vacuous owning-emitter admission plus zero upstream snapshot exits.
#[test]
fn live_repo_has_one_owned_snapshot_publication_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let hits = gts_hits(root);
    let mut report = RepoStaticReport::default();
    check_owned_snapshot_publication(root, &hits, &mut report);
    assert!(report.ok(), "{:?}", report.errors);
}

/// Seal B on the LIVE tree: zero production callers outside the profile crate,
/// across the WHOLE pinned entry-point surface.
#[test]
fn live_repo_has_no_gts_authorship_bypass_outside_the_profile_crate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let bypasses: Vec<GtsAuthorshipHit> = gts_hits(root)
        .into_iter()
        .filter(|hit| !hit.file.starts_with(GTS_PROFILE_CRATE_SRC))
        .collect();
    assert!(bypasses.is_empty(), "{bypasses:?}");
}

/// `crates/*/tests/**` integration tests are NOT production and carry no
/// `#[cfg(test)]` marker at all. Several of them legitimately call the pinned
/// entry points directly (codec-level fixtures); the seals must ignore every
/// one, and this test proves those files really do exist so the exemption is
/// exercised rather than hypothetical.
#[test]
fn integration_test_callers_exist_and_are_outside_the_production_census() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let mut report = RepoStaticReport::default();
    let mut callers: BTreeSet<String> = BTreeSet::new();
    let crates_dir = root.join("crates");
    let mut crate_dirs: Vec<PathBuf> = fs::read_dir(&crates_dir)
        .expect("read crates/")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    crate_dirs.sort();
    for crate_dir in crate_dirs {
        let tests = crate_dir.join("tests");
        if !tests.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_rust_files(&tests, &mut report, &mut files);
        for path in files {
            let text = fs::read_to_string(&path).expect("read integration test");
            let code = blank_comments_strings_and_cfg_test_modules(&text);
            let calls = PURRDF_GTS_ENTRY_POINTS.iter().any(|entry| {
                if entry.constructors.is_empty() {
                    identifier_starts(&code, &format!("{}::{}(", entry.module, entry.item)) > 0
                } else {
                    entry.constructors.iter().any(|ctor| {
                        identifier_starts(
                            &code,
                            &format!("{}::{}::{ctor}(", entry.module, entry.item),
                        ) > 0
                    })
                }
            });
            if calls {
                callers.insert(slash_path(path.strip_prefix(root).unwrap_or(&path)));
            }
        }
    }
    assert!(
        callers.len() >= 6,
        "the crates/*/tests/** exemption must be exercised by real files; found {callers:?}"
    );
    let production: BTreeSet<String> = gts_hits(root).into_iter().map(|hit| hit.file).collect();
    for caller in &callers {
        assert!(
            !production.contains(caller),
            "{caller} is an integration test, not production"
        );
    }
}

#[test]
fn seals_pass_with_only_the_profile_crate_emitter() {
    let temp = tempfile::tempdir().unwrap();
    write_gts_profile_crate(temp.path());
    assert!(gts_seal_errors(temp.path()).is_empty());
}

#[test]
fn seal_a_refuses_borrowed_unchecked_or_receipt_discarding_snapshot_exits() {
    for (before, after) in [
        ("builder: SnapshotBuilder", "builder: &SnapshotBuilder"),
        ("builder.poison()", "other.poison()"),
        ("builder.ingest_totals()", "other.ingest_totals()"),
        ("Result<GmeowGtsEmission>", "Result<Vec<u8>>"),
        (
            "fn emit_snapshot_payload_with_medium",
            "pub fn emit_snapshot_payload_with_medium",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        write_gts_profile_crate(temp.path());
        let path = temp.path().join(GTS_PROFILE_CRATE_SRC).join("lib.rs");
        let original = fs::read_to_string(&path).unwrap();
        assert!(original.contains(before));
        fs::write(&path, original.replace(before, after)).unwrap();
        assert!(
            gts_seal_errors(temp.path())
                .iter()
                .any(|error| error.contains("Seal A")),
            "lost owning publication boundary: {before} -> {after}"
        );
    }
}

#[test]
fn seal_a_fails_on_any_raw_production_emit_gts_caller() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    crate_src(
        root,
        "gmeow-music",
        "lib.rs",
        // gmeow-test-input: synthetic-only
        "pub fn piece_to_gts_bytes() -> Vec<u8> {\n\
             \x20   purrdf::gts_compose::emit_gts(&b, \"dist\", None).unwrap()\n}\n",
    );
    let errs = gts_seal_errors(root);
    assert_eq!(errs.len(), 2, "Seal A and Seal B both fire: {errs:?}");
    assert!(errs[0].contains("Seal A"), "{errs:?}");
    assert!(errs[0].contains("found 1"), "{errs:?}");
}

#[test]
fn seal_b_fails_on_a_writer_with_layout_caller() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    crate_src(
        root,
        "gmeow-pipeline",
        "seg.rs",
        "use purrdf::gts::writer::Writer;\n\
             pub fn seg() -> Vec<u8> {\n\
             \x20   let w = Writer::with_layout(\"ai-package\", Some(\"streamable\"));\n\
             \x20   w.into_bytes()\n}\n",
    );
    let errs = gts_seal_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("Seal B"), "{errs:?}");
    assert!(errs[0].contains("Writer::with_layout("), "{errs:?}");
}

#[test]
fn seal_b_fails_on_a_compact_streamable_caller() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    crate_src(
        root,
        "gmeow-pipeline",
        "compact.rs",
        "pub fn c(data: &[u8]) -> Vec<u8> {\n\
             \x20   purrdf::gts::compact::compact_streamable(data, false).unwrap()\n}\n",
    );
    let errs = gts_seal_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("compact::compact_streamable("), "{errs:?}");
}

/// The three doors purrdf offers that mint a header WITHOUT touching
/// `emit_gts` — an `emit_gts`-only seal is blind to every one of them.
#[test]
fn seal_b_fails_on_to_gts_pack_entries_and_from_tar_callers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    crate_src(
        root,
        "gmeow-pipeline",
        "exit.rs",
        // gmeow-test-input: synthetic-only
        "pub fn a(ds: &RdfDataset) -> Vec<u8> {\n\
             \x20   purrdf::gts_write::to_gts(ds, &look, \"p\").unwrap()\n}\n\
             pub fn b(e: &[FileEntry]) -> Vec<u8> {\n\
             \x20   purrdf::gts::files::pack_entries_v2(e).unwrap()\n}\n\
             pub fn c(d: &[u8]) -> Vec<u8> {\n\
             \x20   purrdf::gts::from_tar::from_tar_bytes(d, &opts).unwrap()\n}\n",
    );
    let errs = gts_seal_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("3 production call(s)"), "{errs:?}");
    // gmeow-test-input: synthetic-only
    assert!(errs[0].contains("gts_write::to_gts("), "{errs:?}");
    assert!(errs[0].contains("files::pack_entries_v2("), "{errs:?}");
    assert!(errs[0].contains("from_tar::from_tar_bytes("), "{errs:?}");
}

/// A `use … as Alias` rename must not hide the call — the real pipeline
/// imported purrdf's `Writer` as `GtsWriter`, so a name-only scan would have
/// missed the very site this work had to fix.
#[test]
fn seal_b_follows_a_renamed_writer_import() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    crate_src(
        root,
        "gmeow-pipeline",
        "mcp.rs",
        "use purrdf::gts::writer::Writer as GtsWriter;\n\
             pub fn seg() -> Vec<u8> {\n\
             \x20   let mut w = GtsWriter::new(\"ai-package\");\n\
             \x20   w.to_bytes()\n}\n",
    );
    let errs = gts_seal_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("GtsWriter::new("), "{errs:?}");
}

/// A renamed FREE function is followed the same way.
#[test]
fn seal_b_follows_a_renamed_free_function_import() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    crate_src(
        root,
        "gmeow-math",
        "lib.rs",
        "use purrdf::gts_write::to_gts as serialize;\n\
             pub fn go(ds: &RdfDataset) -> Vec<u8> {\n\
             \x20   serialize(ds, &look, \"p\").unwrap()\n}\n",
    );
    let errs = gts_seal_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("serialize("), "{errs:?}");
}

/// NON-VACUITY guard #1: the detector must not fire on prose, on a
/// commented-out call, on a call inside a string literal, or on a
/// `#[cfg(test)]` / composed-`cfg` test module.
#[test]
fn seals_ignore_comments_strings_and_test_gated_modules() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    crate_src(
        root,
        "gmeow-pipeline",
        "clean.rs",
        // gmeow-test-input: synthetic-only
        "//! This used to call purrdf::gts_compose::emit_gts(&b) directly.\n\
             // let _ = purrdf::gts_write::to_gts(ds, &look, \"p\");\n\
             pub const HINT: &str = \"route through emit_gts( instead of Writer::new(\";\n\
             pub fn ok() {}\n\
             #[cfg(test)]\n\
             mod tests {\n\
             // gmeow-test-input: synthetic-only
             \x20   fn t() {\n\
             \x20       let _ = purrdf::gts_compose::emit_gts(&b, \"dist\", None);\n\
             \x20       let mut w = purrdf::gts::writer::Writer::new(\"generic\");\n\
             \x20   }\n\
             }\n\
             #[cfg(all(test, not(target_arch = \"wasm32\")))]\n\
             mod native_tests {\n\
             \x20   fn t() {\n\
             \x20       let _ = purrdf::gts::compact::compact_streamable(d, false);\n\
             \x20   }\n\
             }\n",
    );
    let hits = gts_hits(root);
    let outside: Vec<&GtsAuthorshipHit> = hits
        .iter()
        .filter(|hit| !hit.file.starts_with(GTS_PROFILE_CRATE_SRC))
        .collect();
    assert!(outside.is_empty(), "{outside:?}");
    assert!(gts_seal_errors(root).is_empty());
}

/// NON-VACUITY guard #2: substring collisions must not fire. `OkfWriter::new`
/// and `csv::Writer::new` are not purrdf's `Writer`; `pack_to_writer` merely
/// CONTAINS `to_writer`; `emit_gts_report` merely contains `emit_gts`.
#[test]
fn seals_ignore_substring_collisions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_gts_profile_crate(root);
    crate_src(
        root,
        "gmeow-docs",
        "okf.rs",
        "use crate::okf::OkfWriter;\n\
             use csv::Writer;\n\
             pub fn a() {\n\
             \x20   let mut w = OkfWriter::new(config);\n\
             \x20   let mut c = Writer::new(sink);\n\
             \x20   let _ = local_pack_to_writer(&sources, out);\n\
             \x20   let _ = emit_gts_report(&b);\n\
             \x20   let _ = my_from_tar(d);\n}\n",
    );
    let hits = gts_hits(root);
    let outside: Vec<&GtsAuthorshipHit> = hits
        .iter()
        .filter(|hit| !hit.file.starts_with(GTS_PROFILE_CRATE_SRC))
        .collect();
    assert!(outside.is_empty(), "{outside:?}");
}

/// The `csv::Writer` above is a NON-purrdf import, so the alias machinery must
/// not bind it. A purrdf import of the SAME bare name must still bind — this
/// pins that the discrimination is on the `use` path, not on the name.
#[test]
fn only_a_purrdf_use_statement_binds_the_writer_name() {
    assert!(purrdf_use_bindings("use csv::Writer;", "Writer").is_empty());
    assert_eq!(
        purrdf_use_bindings("use purrdf::gts::writer::Writer;", "Writer")
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["Writer".to_string()]
    );
    assert_eq!(
        purrdf_use_bindings("use purrdf::gts::writer::Writer as GtsWriter;", "Writer")
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["GtsWriter".to_string()]
    );
    // A longer identifier that merely CONTAINS the item name binds nothing.
    assert!(
        purrdf_use_bindings("use purrdf::gts::native_codecs::okf::OkfWriter;", "Writer").is_empty()
    );
}

#[test]
fn cfg_predicate_test_gating_is_recognised_in_composed_forms() {
    assert!(cfg_predicate_is_test_gated("test"));
    assert!(cfg_predicate_is_test_gated(
        "all(test, not(target_arch = \"wasm32\"))"
    ));
    assert!(cfg_predicate_is_test_gated(
        "any(test, feature = \"harness\")"
    ));
    assert!(!cfg_predicate_is_test_gated("not(test)"));
    assert!(!cfg_predicate_is_test_gated("feature = \"testing\""));
    assert!(!cfg_predicate_is_test_gated("target_arch = \"wasm32\""));
}

/// The blanker must blank a COMPOSED test-gate's body, not just the bare
/// `#[cfg(test)]` — that hole is what let a wasm-gated test module look like
/// production code to every gate built on this view.
#[test]
fn blank_pass_blanks_a_composed_cfg_test_module_body() {
    // gmeow-test-input: synthetic-only
    let text = "fn prod() {}\n\
                    #[cfg(all(test, not(target_arch = \"wasm32\")))]\n\
                    mod tests {\n    fn t() { purrdf::gts_write::to_gts(x); }\n}\n";
    let code = blank_comments_strings_and_cfg_test_modules(text);
    assert!(code.contains("fn prod"), "{code}");
    assert!(!code.contains("to_gts"), "{code}");
    assert_eq!(code.lines().count(), text.lines().count());
}

// ── the diagnostic-kind ↔ ontology failure-class binding ────────────────

/// A slice Turtle declaring exactly the failure classes `classes` are raised by,
/// wired through `gmeow:enforcesFailureClass` the way the live gts slice is.
fn write_failure_class_slice(root: &Path, classes: &[&str]) {
    let mut ttl = String::from(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
    );
    for class in classes {
        ttl.push_str(&format!(
            "gmeow:{class} a logic:Category .\n\
                 logic:{class}Constraint a logic:Constraint ; \
                 gmeow:enforcesFailureClass gmeow:{class} .\n"
        ));
    }
    write(&root.join("slices/core/gts/module.ttl"), &ttl);
}

/// A `define_diag_kind!` invocation in the exact shape the census reads.
fn diag_kind_source(name: &str, code: &str, failure_class: Option<&str>) -> String {
    let clause = failure_class
        .map(|iri| format!("    failure_class = \"{iri}\";\n"))
        .unwrap_or_default();
    format!(
        "define_diag_kind! {{\n\
             \x20   /// A kind.\n\
             \x20   pub struct {name} {{ detail: String }}\n\
             \x20   code = \"{code}\";\n\
             \x20   grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);\n\
             \x20   message = \"{{}}\", detail;\n\
             {clause}}}\n"
    )
}

fn failure_class_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_diag_failure_class_binding(root, &mut report);
    report.errors
}

/// The census must read a MULTI-LINE struct body correctly: the field list's own
/// closing brace is not the end of the invocation, and treating it as one would
/// silently drop the kind from both gates.
#[test]
fn census_reads_code_and_failure_class_through_a_multiline_struct_body() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-pipeline",
        "error.rs",
        "define_diag_kind! {\n\
             \x20   /// A kind whose field list spans several lines.\n\
             \x20   pub struct Wide {\n\
             \x20       stage: String,\n\
             \x20       rdf: Vec<String>,\n\
             \x20   }\n\
             \x20   code = \"pipeline.wide\";\n\
             \x20   message = \"stage {}: rdf {:?}\", stage, rdf;\n\
             \x20   failure_class = \"https://blackcatinformatics.ca/gmeow/MediumWide\";\n\
             }\n",
    );
    let mut report = RepoStaticReport::default();
    let decls = diag_kind_census(root, &mut report);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(
        decls,
        vec![DiagKindDecl {
            file: "crates/gmeow-pipeline/src/error.rs".to_string(),
            code: "pipeline.wide".to_string(),
            failure_class: Some("https://blackcatinformatics.ca/gmeow/MediumWide".to_string()),
        }]
    );
}

/// A kind bound to an IRI the ontology never minted is a claim about a gate that
/// does not exist — the first half of the bijection.
#[test]
fn a_kind_bound_to_an_unminted_failure_class_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_failure_class_slice(root, &["MediumUnknownSchema"]);
    crate_src(
        root,
        "gmeow-pipeline",
        "error.rs",
        &format!(
            "{}{}",
            diag_kind_source(
                "UnknownSchema",
                "pipeline.medium.unknown-schema",
                Some("https://blackcatinformatics.ca/gmeow/MediumUnknownSchema"),
            ),
            diag_kind_source(
                "Invented",
                "pipeline.medium.invented",
                Some("https://blackcatinformatics.ca/gmeow/MediumInvented"),
            ),
        ),
    );
    let errors = failure_class_errors(root);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("pipeline.medium.invented") && e.contains("MediumInvented")),
        "{errors:?}"
    );
}

/// A `gmeow:Medium*` failure class nobody raises is documentation, not a gate —
/// the second half of the bijection, and the direction a pure Rust-side test
/// could never see.
#[test]
fn a_medium_failure_class_with_no_rust_producer_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_failure_class_slice(root, &["MediumUnknownSchema", "MediumOrphaned"]);
    crate_src(
        root,
        "gmeow-pipeline",
        "error.rs",
        &diag_kind_source(
            "UnknownSchema",
            "pipeline.medium.unknown-schema",
            Some("https://blackcatinformatics.ca/gmeow/MediumUnknownSchema"),
        ),
    );
    let errors = failure_class_errors(root);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("MediumOrphaned") && e.contains("NO Rust producer")),
        "{errors:?}"
    );
}

/// Two producers for one failure class makes "which code raised this" unanswerable.
#[test]
fn two_rust_producers_for_one_medium_failure_class_fail() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_failure_class_slice(root, &["MediumUnknownSchema"]);
    crate_src(
        root,
        "gmeow-pipeline",
        "error.rs",
        &format!(
            "{}{}",
            diag_kind_source(
                "UnknownSchemaA",
                "pipeline.medium.unknown-schema",
                Some("https://blackcatinformatics.ca/gmeow/MediumUnknownSchema"),
            ),
            diag_kind_source(
                "UnknownSchemaB",
                "pipeline.medium.unknown-schema-again",
                Some("https://blackcatinformatics.ca/gmeow/MediumUnknownSchema"),
            ),
        ),
    );
    let errors = failure_class_errors(root);
    assert!(
        errors.iter().any(|e| e.contains("MediumUnknownSchema")
            && e.contains("Rust kinds declare this failure class")),
        "{errors:?}"
    );
}

/// The shrink-only ratchet: a NEW kind carrying no `failure_class` and absent
/// from the pin reds. Without this the annotation stays permanently optional and
/// the bijection is vacuous for every kind but the annotated few.
#[test]
fn a_new_kind_without_a_failure_class_fails_the_ratchet() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_failure_class_slice(root, &[]);
    crate_src(
        root,
        "gmeow-pipeline",
        "error.rs",
        &diag_kind_source("Freshly", "pipeline.freshly-invented", None),
    );
    let mut report = RepoStaticReport::default();
    let decls = diag_kind_census(root, &mut report);
    check_diag_failure_class_ratchet(&decls, &mut report);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("pipeline.freshly-invented")
                && e.contains("PINNED_DIAG_KINDS_WITHOUT_FAILURE_CLASS")),
        "{:?}",
        report.errors
    );
}

/// SHRINKAGE never reds: annotating a pinned kind (so it leaves the live census)
/// without trimming its pin entry must still pass — subset-or-equal, exactly as
/// the `shapes.ttl` ratchet does it.
#[test]
fn annotating_a_pinned_kind_without_trimming_the_pin_still_passes() {
    let pinned = PINNED_DIAG_KINDS_WITHOUT_FAILURE_CLASS
        .first()
        .expect("the pin is non-empty on the live tree");
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_failure_class_slice(root, &["MediumUnknownSchema"]);
    crate_src(
        root,
        "gmeow-pipeline",
        "error.rs",
        &diag_kind_source(
            "NowAnnotated",
            pinned,
            Some("https://blackcatinformatics.ca/gmeow/MediumUnknownSchema"),
        ),
    );
    let mut report = RepoStaticReport::default();
    let decls = diag_kind_census(root, &mut report);
    check_diag_failure_class_ratchet(&decls, &mut report);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

/// Every bound kind is bound on the LIVE tree: the census finds exactly the codes
/// listed below carrying a `failure_class`, and each names a real
/// `gmeow:enforcesFailureClass` individual. A non-vacuity guard for the live-repo
/// gate — if the scanner silently stopped reading a crate's `error.rs`, every
/// assertion above would still pass on a synthetic fixture.
///
/// The list GROWS as the shrink-only census
/// ([`PINNED_DIAG_KINDS_WITHOUT_FAILURE_CLASS`]) shrinks: the two move in lockstep,
/// one entry leaving the pin for every kind that lands here, so a diff that adds a
/// code here without deleting it there (or vice versa) is visible at review.
#[test]
fn the_live_bound_kinds_resolve_to_their_ontology_classes() {
    let root = live_repo_root();
    let mut report = RepoStaticReport::default();
    let decls = diag_kind_census(root, &mut report);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let bound: BTreeMap<&str, &str> = decls
        .iter()
        .filter_map(|d| Some((d.code.as_str(), d.failure_class.as_deref()?)))
        .collect();
    assert_eq!(
        bound.keys().copied().collect::<Vec<_>>(),
        vec![
            "bundle-import.cache",
            "bundle-view.export",
            "bundle-view.io",
            "bundle-view.rdf.parse",
            "docs-catalog.concept-lattice",
            "docs-catalog.distribution",
            "gts-profile.frame",
            "lang-bridge.gmn1.unpinned-glyph-cost",
            "math.lift.empty-codomain",
            "math.lift.onnx.unliftable",
            "math.lift.onnx.wire",
            "math.lift.proof.parse",
            "math.lift.proof.unliftable",
            "math.lift.r.parse",
            "math.lift.r.unliftable",
            "math.lift.source.not-utf8",
            "mcp-dev.error",
            "mcp.duplicate-registration",
            "mcp.invalid-registration",
            "mcp.medium.unpinned-store-dictionary",
            "mcp.segment-not-loaded",
            "mcp.unknown-resource",
            "mcp.unknown-tool",
            "pipeline.medium.corpus-drift",
            "pipeline.medium.dictionary-regression",
            "pipeline.medium.digest-mismatch",
            "pipeline.medium.opaque-frame",
            "pipeline.medium.undeclared-dictionary",
            "pipeline.medium.unknown-dictionary",
            "pipeline.medium.unknown-schema",
            "slice-quality.record",
        ],
        "these are the failure-class-bound kinds today"
    );
    let declared = ontology_failure_classes(root, &mut report);
    for (code, iri) in bound {
        assert!(
            declared.contains(iri),
            "{code} binds <{iri}>, which no slice raises through gmeow:enforcesFailureClass"
        );
    }
}

// ── temp-directory hygiene: every temporary path is RAII-managed ──────

fn temp_dir_errors(root: &Path) -> Vec<String> {
    let mut report = RepoStaticReport::default();
    check_no_unmanaged_temp_dir(root, &mut report);
    report.errors
}

/// The regression this whole gate exists for: the LIVE repository must contain zero
/// hand-rolled system-temp scratch paths. This is the check that actually stops the
/// leak from growing back — the synthetic cases below only prove the scanner has teeth.
#[test]
fn live_tree_has_no_unmanaged_temp_dir() {
    let errs = temp_dir_errors(live_repo_root());
    assert!(
        errs.is_empty(),
        "the live tree must build every temporary path through tempfile's RAII guards; \
             {} site(s) hand-roll one instead: {errs:#?}",
        errs.len()
    );
}

#[test]
fn hand_rolled_temp_dir_in_production_code_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "fn scratch() -> PathBuf {\n    \
                 std::env::temp_dir().join(format!(\"gmeow-x-{}\", std::process::id()))\n}\n",
    );
    let errs = temp_dir_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("tempfile::TempDir"), "{errs:?}");
    assert!(
        errs[0].contains("crates/gmeow-foo/src/lib.rs:2"),
        "{errs:?}"
    );
}

/// The leaks that filled the host's `/tmp` were almost all inside `#[cfg(test)]`
/// modules, so — unlike the `Result<_, String>` scan — this gate must NOT exempt test
/// bodies. A gate that ignored them would have caught none of the original 72 sites.
#[test]
fn hand_rolled_temp_dir_inside_cfg_test_module_still_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "fn real() {}\n\
             #[cfg(test)]\nmod tests {\n    \
                 fn fixture() -> PathBuf { std::env::temp_dir().join(\"gmeow-y\") }\n}\n",
    );
    let errs = temp_dir_errors(root);
    assert_eq!(errs.len(), 1, "{errs:?}");
}

/// The `use std::env;`-shortened spelling is the same defect and must not be a bypass.
/// It is reported ONCE, not twice, even though `std::env::temp_dir` contains
/// `env::temp_dir` as a substring.
#[test]
fn shortened_env_temp_dir_spelling_fails_once() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "use std::env;\nfn s() -> PathBuf { env::temp_dir().join(\"gmeow-z\") }\n",
    );
    let errs = temp_dir_errors(root);
    assert_eq!(
        errs.len(),
        1,
        "the shortened spelling must fail exactly once: {errs:?}"
    );

    let temp2 = tempfile::tempdir().unwrap();
    crate_src(
        temp2.path(),
        "gmeow-foo",
        "lib.rs",
        "fn s() -> PathBuf { std::env::temp_dir().join(\"gmeow-z\") }\n",
    );
    let errs2 = temp_dir_errors(temp2.path());
    assert_eq!(
        errs2.len(),
        1,
        "the qualified spelling must not double-report via its own substring: {errs2:?}"
    );
}

/// Disarming an RAII guard re-creates the leak, so `.into_path()` / `.keep()` are banned
/// too — otherwise the gate would be trivially bypassable by one method call.
#[test]
fn disarming_an_raii_temp_guard_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "fn a() -> PathBuf { tempfile::tempdir().unwrap().into_path() }\n\
             fn b() -> PathBuf { tempfile::tempdir().unwrap().keep() }\n",
    );
    let errs = temp_dir_errors(root);
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(
        errs.iter().all(|e| e.contains("disarms its Drop")),
        "{errs:?}"
    );
}

/// Prose must never trip the gate: a comment or a string literal that NAMES the banned
/// accessor (this gate's own doc comments and error text do exactly that) is not a use.
#[test]
fn temp_dir_named_in_comment_or_string_literal_is_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "// Never call std::env::temp_dir() by hand; use tempfile::TempDir.\n\
             /// Doc: `std::env::temp_dir()` is banned.\n\
             const MSG: &str = \"do not use std::env::temp_dir()\";\n\
             fn ok() { let _tmp = tempfile::tempdir().unwrap(); }\n",
    );
    let errs = temp_dir_errors(root);
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn raii_tempdir_usage_passes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate_src(
        root,
        "gmeow-foo",
        "lib.rs",
        "fn fixture() {\n    \
                 let tmp = tempfile::tempdir().expect(\"tempdir\");\n    \
                 let case = tmp.path().join(\"foundation\").join(\"free-role\");\n    \
                 std::fs::create_dir_all(&case).unwrap();\n}\n",
    );
    let errs = temp_dir_errors(root);
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn temp_dir_gate_skips_a_repo_without_crates() {
    let temp = tempfile::tempdir().unwrap();
    assert!(temp_dir_errors(temp.path()).is_empty());
}
