// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn write_crate(crates_dir: &Path, name: &str, deps: &[(&str, &str)], registry: &[(&str, &str)]) {
    let crate_dir = crates_dir.join(name);
    fs::create_dir_all(&crate_dir).expect("crate dir should be created");
    let mut lines = vec![
        "[package]".to_owned(),
        format!("name = \"{name}\""),
        "version = \"0.1.0\"".to_owned(),
        String::new(),
        "[dependencies]".to_owned(),
    ];
    for (dep_name, dep_dir) in deps {
        lines.push(format!("{dep_name} = {{ path = \"../{dep_dir}\" }}"));
    }
    for (dep_name, version) in registry {
        lines.push(format!("{dep_name} = \"{version}\""));
    }
    fs::write(crate_dir.join("Cargo.toml"), lines.join("\n") + "\n")
        .expect("manifest should be written");
}

fn write_rdf_stack(crates_dir: &Path) {
    write_crate(crates_dir, RDF_EVENTS_CRATE, &[], &[]);
    write_crate(
        crates_dir,
        KERNEL_CRATE,
        &[(RDF_EVENTS_CRATE, RDF_EVENTS_CRATE)],
        &[],
    );
    write_crate(
        crates_dir,
        RDF_ADAPTER_CRATE,
        &[(KERNEL_CRATE, KERNEL_CRATE)],
        &[],
    );
}

#[test]
fn live_workspace_passes() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("validate crate should live under crates/");
    let report = check_crate_layering(crates_dir);
    // The live gmeow workspace is acyclic and every first-party dependency
    // resolves to a `crates/*` member. There are no RDF-crate-topology
    // assertions (kernel/events/adapter edges): those crates live in the sibling
    // `purrdf` package, not this workspace.
    assert!(report.ok(), "{:?}", report.errors);
    assert!(
        !report.edges.is_empty(),
        "the live workspace must contribute crate edges"
    );
}

#[test]
fn registry_dep_is_not_first_party() {
    let temp = tempfile::tempdir().unwrap();
    let crates = temp.path().join("crates");
    write_crate(&crates, RDF_EVENTS_CRATE, &[], &[]);
    write_crate(
        &crates,
        KERNEL_CRATE,
        &[(RDF_EVENTS_CRATE, RDF_EVENTS_CRATE)],
        &[("gmeow-gts", "0.9.5")],
    );
    write_crate(
        &crates,
        RDF_ADAPTER_CRATE,
        &[(KERNEL_CRATE, KERNEL_CRATE)],
        &[],
    );
    let report = check_crate_layering(&crates);
    assert!(report.ok(), "{:?}", report.errors);
    assert_eq!(
        report.edges.get(KERNEL_CRATE),
        Some(&BTreeSet::from([RDF_EVENTS_CRATE.to_owned()]))
    );
}

#[test]
fn cycle_is_detected() {
    let temp = tempfile::tempdir().unwrap();
    let crates = temp.path().join("crates");
    write_rdf_stack(&crates);
    write_crate(&crates, "gmeow-a", &[("gmeow-b", "gmeow-b")], &[]);
    write_crate(&crates, "gmeow-b", &[("gmeow-a", "gmeow-a")], &[]);
    let report = check_crate_layering(&crates);
    assert!(!report.ok());
    let cycle_errors = report
        .errors
        .iter()
        .filter(|e| e.contains("cycle"))
        .collect::<Vec<_>>();
    assert!(!cycle_errors.is_empty());
    assert!(cycle_errors[0].contains("gmeow-a"));
    assert!(cycle_errors[0].contains("gmeow-b"));
}

#[test]
fn dangling_path_edge_fails() {
    let temp = tempfile::tempdir().unwrap();
    let crates = temp.path().join("crates");
    write_rdf_stack(&crates);
    write_crate(
        &crates,
        "gmeow-a",
        &[("gmeow-missing", "gmeow-missing")],
        &[],
    );
    let report = check_crate_layering(&crates);
    assert!(!report.ok());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("not a crates/* member"))
    );
}

#[test]
fn renamed_package_path_dep_is_first_party() {
    let temp = tempfile::tempdir().unwrap();
    let crates = temp.path().join("crates");
    fs::create_dir_all(&crates).unwrap();
    write_crate(&crates, RDF_EVENTS_CRATE, &[], &[]);
    write_crate(
        &crates,
        RDF_ADAPTER_CRATE,
        &[(KERNEL_CRATE, KERNEL_CRATE)],
        &[],
    );
    write_crate(&crates, "gmeow-errors", &[], &[]);
    let kernel_dir = crates.join(KERNEL_CRATE);
    fs::create_dir_all(&kernel_dir).unwrap();
    fs::write(
        kernel_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{KERNEL_CRATE}\"\nversion = \"0.1.0\"\n\n\
                 [dependencies]\n{RDF_EVENTS_CRATE} = {{ path = \"../{RDF_EVENTS_CRATE}\" }}\n\
                 aliased = {{ path = \"../gmeow-errors\", package = \"gmeow-errors\" }}\n"
        ),
    )
    .unwrap();
    let report = check_crate_layering(&crates);
    // A `package = "..."`-aliased path dependency is recognized as first-party
    // by its resolved package name (`gmeow-errors`), so it becomes a real
    // graph edge. No RDF-core-purity rule constrains it (that layering is the
    // sibling `purrdf` package's concern), so this edge does not trip a gate.
    assert!(report.ok(), "{:?}", report.errors);
    assert_eq!(
        report.edges.get(KERNEL_CRATE),
        Some(&BTreeSet::from([
            RDF_EVENTS_CRATE.to_owned(),
            "gmeow-errors".to_owned(),
        ]))
    );
}

#[test]
fn target_table_path_dep_is_first_party() {
    let temp = tempfile::tempdir().unwrap();
    let crates = temp.path().join("crates");
    fs::create_dir_all(&crates).unwrap();
    write_rdf_stack(&crates);
    write_crate(&crates, "gmeow-b", &[], &[]);
    let crate_dir = crates.join("gmeow-a");
    fs::create_dir_all(&crate_dir).unwrap();
    fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"gmeow-a\"\nversion = \"0.1.0\"\n\n\
             [target.'cfg(unix)'.dependencies]\ngmeow-b = { path = \"../gmeow-b\" }\n",
    )
    .unwrap();
    let report = check_crate_layering(&crates);
    assert!(report.ok(), "{:?}", report.errors);
    assert_eq!(
        report.edges.get("gmeow-a"),
        Some(&BTreeSet::from(["gmeow-b".to_owned()]))
    );
}

#[test]
fn diagnostics_projection_carries_errors() {
    // A still-enforced violation (a first-party dep that resolves to no
    // `crates/*` member) must surface through the diagnostics projection.
    let temp = tempfile::tempdir().unwrap();
    let crates = temp.path().join("crates");
    write_crate(
        &crates,
        "gmeow-a",
        &[("gmeow-missing", "gmeow-missing")],
        &[],
    );
    let report = check_crate_layering(&crates);
    assert!(!report.ok());
    let diagnostics = to_diagnostics_report(&report);
    assert_eq!(diagnostics.findings.len(), report.errors.len());
    assert!(
        diagnostics
            .findings
            .iter()
            .any(|finding| finding.code == "crate-layering.violation")
    );
}
