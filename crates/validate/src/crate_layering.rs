// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust crate-layering gate: an acyclic first-party crate DAG.
//!
//! Enforces that gmeow's own `crates/*` graph is acyclic and that every
//! first-party dependency resolves to a `crates/*` member. First-party
//! dependencies are `gmeow-*` crates declared with a local `path`; registry
//! crates (including the external `purrdf` umbrella) are external boundaries and
//! do not become internal layering edges.
//!
//! The RDF-1.2 kernel/adapter/event-seam crates are not gmeow's: the RDF 1.2 stack
//! is the external `purrdf` toolkit, which owns and gates that layering internally.
//! gmeow consumes it through the single `purrdf` umbrella, so those crates are not
//! `crates/*` members and this gate does not police their purity. The
//! `KERNEL_CRATE` / `RDF_*` constants below are generic crate-name fixtures for
//! this module's unit tests only.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use gmeow_errors::{Finding, Report, Severity};
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

fn manifest_paths(crates_dir: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let entries = fs::read_dir(crates_dir).map_err(|err| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!(
                "cannot read crates directory {}: {err}",
                crates_dir.display()
            ),
        })
    })?;
    let mut manifests = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                detail: format!(
                    "cannot read crates directory {}: {err}",
                    crates_dir.display()
                ),
            })
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
        Err(diag) => {
            report.errors.push(diag.message().to_owned());
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

    // The RDF-1.2 kernel / event-seam / adapter layering discipline (former S0
    // P2b: gmeow-rdf-core purity, gmeow-rdf-events zero-dep seam, gmeow-rdf
    // adapter) is not gmeow's to enforce: the RDF stack is the external `purrdf`
    // toolkit, which owns and gates that layering internally. gmeow consumes it
    // through the single `purrdf` umbrella dependency, so those crates are not
    // `crates/*` members here. The general first-party-dependency-resolves check
    // below still applies to gmeow's own crate graph.

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
            Finding::new(
                Severity::Error,
                crate::codes::CRATE_LAYERING_VIOLATION,
                message.clone(),
            )
            .with_tool(TOOL),
        );
    }
    for message in &report.warnings {
        out.add_finding(
            Finding::new(
                Severity::Warning,
                crate::codes::CRATE_LAYERING_OBSERVATION,
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
}
