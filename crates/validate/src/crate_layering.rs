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

use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, GateVerdict, Grade, Location, Report, Severity, StageId,
    Standpoint, register_code,
};
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
#[derive(Debug, Clone, Default)]
pub struct CrateLayeringReport {
    ledger: DiagLedger,
    pub edges: BTreeMap<String, BTreeSet<String>>,
}

impl CrateLayeringReport {
    pub fn ok(&self) -> bool {
        self.ledger.verdict() == GateVerdict::Collected
    }

    pub fn ledger(&self) -> &DiagLedger {
        &self.ledger
    }

    pub fn errors(&self) -> Vec<String> {
        self.ledger
            .messages(Severity::Error)
            .map(str::to_owned)
            .collect()
    }
}

#[track_caller]
fn violation(path: &str, focus: &str, message: impl Into<String>) -> Diag {
    Diag::new(
        register_code(crate::codes::CRATE_LAYERING_VIOLATION),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    )
    .with_location(Location::new(Some(path.to_owned()), None, None, None))
    .with_focus(focus)
}

fn source_path(crates_dir: &Path, path: &Path) -> String {
    path.strip_prefix(crates_dir.parent().unwrap_or(crates_dir))
        .expect("crate-layering source must belong to its selected source root")
        .to_str()
        .expect("crate-layering source path must be valid UTF-8")
        .replace('\\', "/")
}

fn layering_stage() -> StageId {
    StageId::new("validate.crate-layering")
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
    let source = source_path(crates_dir, crates_dir);
    let entries = fs::read_dir(crates_dir).map_err(|err| {
        violation(
            &source,
            "directory",
            format!(
                "cannot read crates directory {}: {err}",
                crates_dir.display()
            ),
        )
        .with_source(err)
    })?;
    let mut manifests = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| {
            violation(
                &source,
                "directory",
                format!(
                    "cannot read crates directory {}: {err}",
                    crates_dir.display()
                ),
            )
            .with_source(err)
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
    let directory = source_path(crates_dir, crates_dir);

    if !crates_dir.is_dir() {
        report.ledger.attach(
            violation(
                &directory,
                "directory",
                format!("crates directory not found: {}", crates_dir.display()),
            ),
            layering_stage(),
        );
        return report;
    }

    let manifests = match manifest_paths(crates_dir) {
        Ok(paths) => paths,
        Err(diag) => {
            report.ledger.attach(diag, layering_stage());
            return report;
        }
    };
    if manifests.is_empty() {
        report.ledger.attach(
            violation(
                &directory,
                "directory",
                format!("no crates/*/Cargo.toml under {}", crates_dir.display()),
            ),
            layering_stage(),
        );
        return report;
    }

    let mut manifest_sources = BTreeMap::new();
    for manifest_path in manifests {
        let source = source_path(crates_dir, &manifest_path);
        let text = match fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(err) => {
                report.ledger.attach(
                    violation(
                        &source,
                        "manifest",
                        format!("{}: cannot read Cargo.toml: {err}", manifest_path.display()),
                    )
                    .with_source(err),
                    layering_stage(),
                );
                continue;
            }
        };
        let manifest = match text.parse::<Value>() {
            Ok(manifest) => manifest,
            Err(err) => {
                report.ledger.attach(
                    violation(
                        &source,
                        "manifest",
                        format!(
                            "{}: cannot parse Cargo.toml: {err}",
                            manifest_path.display()
                        ),
                    )
                    .with_source(err),
                    layering_stage(),
                );
                continue;
            }
        };
        let Some(name) = crate_name(&manifest) else {
            report.ledger.attach(
                violation(
                    &source,
                    "package.name",
                    format!("{}: no [package] name", manifest_path.display()),
                ),
                layering_stage(),
            );
            continue;
        };
        if manifest_sources.contains_key(name) {
            report.ledger.attach(
                violation(
                    &source,
                    name,
                    format!(
                        "duplicate crate name {name:?} in {}",
                        manifest_path.display()
                    ),
                ),
                layering_stage(),
            );
            continue;
        }
        manifest_sources.insert(name.to_owned(), source);
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
                report.ledger.attach(
                    violation(
                        &manifest_sources[krate],
                        dep,
                        format!("{krate}: first-party dependency {dep:?} is not a crates/* member"),
                    ),
                    layering_stage(),
                );
            }
        }
    }

    if let Some(cycle) = find_cycle(&report.edges) {
        report.ledger.attach(
            violation(
                &directory,
                &cycle.join(" -> "),
                format!("first-party crate dependency cycle: {}", cycle.join(" -> ")),
            ),
            layering_stage(),
        );
    }

    report
}

/// Project a crate-layering report into the canonical diagnostics model.
pub fn to_diagnostics_report(report: &CrateLayeringReport) -> Report {
    report.ledger.project_report(TOOL)
}

#[path = "crate_layering.tests.rs"]
#[cfg(test)]
mod tests;
