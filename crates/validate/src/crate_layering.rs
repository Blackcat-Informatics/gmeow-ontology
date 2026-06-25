// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust crate-layering gate: RDF core purity plus an acyclic crate DAG.
//!
//! This is the Rust-side enforcement for the crate boundary described in #820
//! S0: `gmeow-rdf-core` is the generic RDF 1.2 kernel, `gmeow-rdf-events` is
//! the neutral event protocol seam, and `gmeow-rdf` is the oxigraph/PyO3 adapter
//! that must depend on and re-export the core. First-party dependencies are
//! `gmeow-*` crates declared with a local `path`; registry crates with the same
//! prefix are external boundaries and do not become internal layering edges.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use gmeow_diagnostics::{Finding, Report, Severity};
use toml::Value;

/// The generic RDF 1.2 kernel. Slice/domain/adapter semantics layer above it.
pub const KERNEL_CRATE: &str = "gmeow-rdf-core";

/// The oxigraph/PyO3 adapter and re-export surface. It must depend on the core.
pub const RDF_ADAPTER_CRATE: &str = "gmeow-rdf";

/// The neutral ingestion protocol seam. It must not depend on either side.
pub const RDF_EVENTS_CRATE: &str = "gmeow-rdf-events";

const FIRST_PARTY_PREFIX: &str = "gmeow-";
const DEP_TABLE_KEYS: &[&str] = &["dependencies", "build-dependencies", "dev-dependencies"];
const TOOL: &str = "crate-layering";

/// Outcome of the crate-layering gate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrateLayeringReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub edges: BTreeMap<String, BTreeSet<String>>,
}

impl CrateLayeringReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

fn crate_name(manifest: &Value) -> Option<&str> {
    manifest
        .get("package")
        .and_then(Value::as_table)
        .and_then(|package| package.get("name"))
        .and_then(Value::as_str)
}

fn dependency_tables(manifest: &Value) -> Vec<&toml::map::Map<String, Value>> {
    let mut tables = Vec::new();
    for key in DEP_TABLE_KEYS {
        if let Some(table) = manifest.get(*key).and_then(Value::as_table) {
            tables.push(table);
        }
    }
    if let Some(targets) = manifest.get("target").and_then(Value::as_table) {
        for cfg_table in targets.values().filter_map(Value::as_table) {
            for key in DEP_TABLE_KEYS {
                if let Some(table) = cfg_table.get(*key).and_then(Value::as_table) {
                    tables.push(table);
                }
            }
        }
    }
    tables
}

fn first_party_deps(manifest: &Value) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    for table in dependency_tables(manifest) {
        for (dep_name, spec) in table {
            let mut effective = dep_name.as_str();
            let mut has_path = false;
            if let Some(spec_table) = spec.as_table() {
                if let Some(package) = spec_table.get("package").and_then(Value::as_str) {
                    effective = package;
                }
                has_path = spec_table.get("path").and_then(Value::as_str).is_some();
            }
            if effective.starts_with(FIRST_PARTY_PREFIX) && has_path {
                deps.insert(effective.to_owned());
            }
        }
    }
    deps
}

fn manifest_paths(crates_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(crates_dir).map_err(|err| {
        format!(
            "cannot read crates directory {}: {err}",
            crates_dir.display()
        )
    })?;
    let mut manifests = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| {
            format!(
                "cannot read crates directory {}: {err}",
                crates_dir.display()
            )
        })?;
        let path = entry.path().join("Cargo.toml");
        if path.is_file() {
            manifests.push(path);
        }
    }
    manifests.sort();
    Ok(manifests)
}

const WHITE: u8 = 0;
const GREY: u8 = 1;
const BLACK: u8 = 2;

fn find_cycle(edges: &BTreeMap<String, BTreeSet<String>>) -> Option<Vec<String>> {
    let mut colour = edges
        .keys()
        .map(|node| (node.clone(), WHITE))
        .collect::<BTreeMap<_, _>>();

    for root in edges.keys() {
        if colour.get(root).copied().unwrap_or(WHITE) != WHITE {
            continue;
        }
        let mut stack = vec![(root.clone(), sorted_deps(edges, root))];
        let mut stack_nodes = vec![root.clone()];
        colour.insert(root.clone(), GREY);

        while let Some((node, pending)) = stack.last_mut() {
            if pending.is_empty() {
                colour.insert(node.clone(), BLACK);
                stack.pop();
                stack_nodes.pop();
                continue;
            }

            let next = pending.remove(0);
            match colour.get(&next).copied().unwrap_or(WHITE) {
                GREY => {
                    let start = stack_nodes
                        .iter()
                        .position(|item| item == &next)
                        .expect("grey node must be on the recursion stack");
                    let mut cycle = stack_nodes[start..].to_vec();
                    cycle.push(next);
                    return Some(cycle);
                }
                WHITE => {
                    colour.insert(next.clone(), GREY);
                    stack.push((next.clone(), sorted_deps(edges, &next)));
                    stack_nodes.push(next);
                }
                BLACK => {}
                _ => unreachable!("unknown DFS colour"),
            }
        }
    }
    None
}

fn sorted_deps(edges: &BTreeMap<String, BTreeSet<String>>, node: &str) -> Vec<String> {
    edges
        .get(node)
        .map(|deps| deps.iter().cloned().collect())
        .unwrap_or_default()
}

/// Run the crate-layering gate over every `crates/*/Cargo.toml`.
pub fn check_crate_layering(crates_dir: &Path) -> CrateLayeringReport {
    let mut report = CrateLayeringReport::default();

    if !crates_dir.is_dir() {
        report.errors.push(format!(
            "crates directory not found: {}",
            crates_dir.display()
        ));
        return report;
    }

    let manifests = match manifest_paths(crates_dir) {
        Ok(paths) => paths,
        Err(message) => {
            report.errors.push(message);
            return report;
        }
    };
    if manifests.is_empty() {
        report.errors.push(format!(
            "no crates/*/Cargo.toml under {}",
            crates_dir.display()
        ));
        return report;
    }

    let mut names_seen = BTreeSet::new();
    for manifest_path in manifests {
        let text = match fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(err) => {
                report.errors.push(format!(
                    "{}: cannot read Cargo.toml: {err}",
                    manifest_path.display()
                ));
                continue;
            }
        };
        let manifest = match text.parse::<Value>() {
            Ok(manifest) => manifest,
            Err(err) => {
                report.errors.push(format!(
                    "{}: cannot parse Cargo.toml: {err}",
                    manifest_path.display()
                ));
                continue;
            }
        };
        let Some(name) = crate_name(&manifest) else {
            report
                .errors
                .push(format!("{}: no [package] name", manifest_path.display()));
            continue;
        };
        if !names_seen.insert(name.to_owned()) {
            report.errors.push(format!(
                "duplicate crate name {name:?} in {}",
                manifest_path.display()
            ));
            continue;
        }
        report
            .edges
            .insert(name.to_owned(), first_party_deps(&manifest));
    }

    match report.edges.get(KERNEL_CRATE) {
        None => report.errors.push(format!(
            "kernel crate {KERNEL_CRATE:?} not found under {}",
            crates_dir.display()
        )),
        Some(kernel_deps) => {
            let allowed = BTreeSet::from([RDF_EVENTS_CRATE.to_owned()]);
            let disallowed = kernel_deps
                .difference(&allowed)
                .cloned()
                .collect::<Vec<_>>();
            if !disallowed.is_empty() {
                report.errors.push(format!(
                    "{KERNEL_CRATE} (the RDF-1.2 core kernel) may only depend on \
                     {RDF_EVENTS_CRATE} first-party support crates, but depends on {} \
                     - slice/domain/adapter semantics must layer ABOVE the core, \
                     never inside it (#820 S0 RDF core purity)",
                    disallowed.join(", ")
                ));
            }
        }
    }

    match report.edges.get(RDF_EVENTS_CRATE) {
        None => report.errors.push(format!(
            "RDF event seam crate {RDF_EVENTS_CRATE:?} not found under {}",
            crates_dir.display()
        )),
        Some(event_deps) if !event_deps.is_empty() => {
            report.errors.push(format!(
                "{RDF_EVENTS_CRATE} (the neutral RDF event protocol seam) must have \
                 ZERO first-party dependencies, but depends on {} (#820 S0 protocol seam purity)",
                event_deps.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        Some(_) => {}
    }

    match report.edges.get(RDF_ADAPTER_CRATE) {
        None => report.errors.push(format!(
            "RDF adapter crate {RDF_ADAPTER_CRATE:?} not found under {}",
            crates_dir.display()
        )),
        Some(adapter_deps) if !adapter_deps.contains(KERNEL_CRATE) => {
            report.errors.push(format!(
                "{RDF_ADAPTER_CRATE} must depend on {KERNEL_CRATE}: the oxigraph/PyO3 \
                 adapter is required to re-export the ring-fenced RDF core (#885 P2b)"
            ));
        }
        Some(_) => {}
    }

    for (krate, deps) in &report.edges {
        for dep in deps {
            if !report.edges.contains_key(dep) {
                report.errors.push(format!(
                    "{krate}: first-party dependency {dep:?} is not a crates/* member"
                ));
            }
        }
    }

    if let Some(cycle) = find_cycle(&report.edges) {
        report.errors.push(format!(
            "first-party crate dependency cycle: {}",
            cycle.join(" -> ")
        ));
    }

    report
}

/// Project a crate-layering report into the canonical diagnostics model.
pub fn to_diagnostics_report(report: &CrateLayeringReport) -> Report {
    let mut out = Report::new(TOOL);
    for message in &report.errors {
        out.add_finding(
            Finding::new(Severity::Error, "crate-layering.violation", message.clone())
                .with_tool(TOOL),
        );
    }
    for message in &report.warnings {
        out.add_finding(
            Finding::new(
                Severity::Warning,
                "crate-layering.observation",
                message.clone(),
            )
            .with_tool(TOOL),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_crate(
        crates_dir: &Path,
        name: &str,
        deps: &[(&str, &str)],
        registry: &[(&str, &str)],
    ) {
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
        assert!(report.ok(), "{:?}", report.errors);
        assert_eq!(report.edges.get(RDF_EVENTS_CRATE), Some(&BTreeSet::new()));
        assert_eq!(
            report.edges.get(KERNEL_CRATE),
            Some(&BTreeSet::from([RDF_EVENTS_CRATE.to_owned()]))
        );
        assert!(report
            .edges
            .get(RDF_ADAPTER_CRATE)
            .is_some_and(|deps| deps.contains(KERNEL_CRATE)));
    }

    #[test]
    fn kernel_must_be_present() {
        let temp = tempfile::tempdir().unwrap();
        let crates = temp.path().join("crates");
        write_crate(&crates, "gmeow-other", &[], &[]);
        let report = check_crate_layering(&crates);
        assert!(!report.ok());
        assert!(report.errors.iter().any(|e| e.contains("not found")));
    }

    #[test]
    fn rdf_core_disallowed_dependency_fails() {
        let temp = tempfile::tempdir().unwrap();
        let crates = temp.path().join("crates");
        write_crate(&crates, RDF_EVENTS_CRATE, &[], &[]);
        write_crate(&crates, "gmeow-diagnostics", &[], &[]);
        write_crate(
            &crates,
            KERNEL_CRATE,
            &[
                (RDF_EVENTS_CRATE, RDF_EVENTS_CRATE),
                ("gmeow-diagnostics", "gmeow-diagnostics"),
            ],
            &[],
        );
        write_crate(
            &crates,
            RDF_ADAPTER_CRATE,
            &[(KERNEL_CRATE, KERNEL_CRATE)],
            &[],
        );
        let report = check_crate_layering(&crates);
        assert!(!report.ok());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("may only depend on")));
    }

    #[test]
    fn rdf_events_must_have_zero_first_party_deps() {
        let temp = tempfile::tempdir().unwrap();
        let crates = temp.path().join("crates");
        write_crate(&crates, "gmeow-diagnostics", &[], &[]);
        write_crate(
            &crates,
            RDF_EVENTS_CRATE,
            &[("gmeow-diagnostics", "gmeow-diagnostics")],
            &[],
        );
        write_crate(
            &crates,
            KERNEL_CRATE,
            &[(RDF_EVENTS_CRATE, RDF_EVENTS_CRATE)],
            &[],
        );
        write_crate(
            &crates,
            RDF_ADAPTER_CRATE,
            &[(KERNEL_CRATE, KERNEL_CRATE)],
            &[],
        );
        let report = check_crate_layering(&crates);
        assert!(!report.ok());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("protocol seam") && e.contains("ZERO first-party")));
    }

    #[test]
    fn rdf_adapter_must_depend_on_core() {
        let temp = tempfile::tempdir().unwrap();
        let crates = temp.path().join("crates");
        write_crate(&crates, RDF_EVENTS_CRATE, &[], &[]);
        write_crate(
            &crates,
            KERNEL_CRATE,
            &[(RDF_EVENTS_CRATE, RDF_EVENTS_CRATE)],
            &[],
        );
        write_crate(&crates, RDF_ADAPTER_CRATE, &[], &[]);
        let report = check_crate_layering(&crates);
        assert!(!report.ok());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("must depend on gmeow-rdf-core")));
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
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("not a crates/* member")));
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
        write_crate(&crates, "gmeow-diagnostics", &[], &[]);
        let kernel_dir = crates.join(KERNEL_CRATE);
        fs::create_dir_all(&kernel_dir).unwrap();
        fs::write(
            kernel_dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{KERNEL_CRATE}\"\nversion = \"0.1.0\"\n\n\
                 [dependencies]\n{RDF_EVENTS_CRATE} = {{ path = \"../{RDF_EVENTS_CRATE}\" }}\n\
                 aliased = {{ path = \"../gmeow-diagnostics\", package = \"gmeow-diagnostics\" }}\n"
            ),
        )
        .unwrap();
        let report = check_crate_layering(&crates);
        assert!(!report.ok());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("may only depend on")));
        assert_eq!(
            report.edges.get(KERNEL_CRATE),
            Some(&BTreeSet::from([
                RDF_EVENTS_CRATE.to_owned(),
                "gmeow-diagnostics".to_owned(),
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
        let temp = tempfile::tempdir().unwrap();
        let crates = temp.path().join("crates");
        write_crate(&crates, RDF_EVENTS_CRATE, &[], &[]);
        write_crate(&crates, "gmeow-diagnostics", &[], &[]);
        write_crate(
            &crates,
            KERNEL_CRATE,
            &[
                (RDF_EVENTS_CRATE, RDF_EVENTS_CRATE),
                ("gmeow-diagnostics", "gmeow-diagnostics"),
            ],
            &[],
        );
        write_crate(
            &crates,
            RDF_ADAPTER_CRATE,
            &[(KERNEL_CRATE, KERNEL_CRATE)],
            &[],
        );
        let report = check_crate_layering(&crates);
        let diagnostics = to_diagnostics_report(&report);
        assert_eq!(diagnostics.findings.len(), report.errors.len());
        assert!(diagnostics
            .findings
            .iter()
            .any(|finding| finding.code == "crate-layering.violation"));
    }
}
