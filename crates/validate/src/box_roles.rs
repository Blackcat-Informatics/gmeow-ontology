// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Graph-box role coverage audit over authored GMEOW sources.
//!
//! PyO3-free. Mirrors `gmeow_tools.box_roles` EXACTLY: it loads the authored
//! Turtle sources, collects every typed GMEOW term, assigns each a single `kind`
//! by priority, and audits explicit `gmeow:graphBoxRole` coverage — reporting a
//! `missing` finding for an untyped term, an `invalid` finding for a role object
//! that is not an IRI or not typed `gmeow:GraphBoxRole`, and otherwise tallying
//! the role into a `gmeow:`-curied distribution.
//!
//! The GMEOW vocabulary namespace itself is NOT a constant here: it is passed in
//! from the Python `config.NAMESPACE` / `config.ONTOLOGY_IRI` single source of
//! truth so the two never drift, matching the rest of this crate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_errors::{Finding, Location, Report, Severity};
use purrdf::{DatasetView, GraphMatch, RdfDatasetBuilder, TermRef, TermValue};

use crate::model::{owl, rdf, rdfs};
use crate::store;

/// One box-role audit finding (the term, its kind, the source file, the message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleFinding {
    /// The full term IRI.
    pub term: String,
    /// The single assigned kind (`ontology`/`class`/…/`individual`).
    pub kind: String,
    /// The source file the term's type-triple first appeared in.
    pub source: String,
    /// The human-facing message for this finding.
    pub message: String,
}

/// Graph-box role coverage report (mirrors `gmeow_tools.box_roles.BoxRoleAudit`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoxRoleAudit {
    /// The number of typed GMEOW terms audited.
    pub term_count: usize,
    /// The `_curie(role) -> count` distribution of valid roles (sorted).
    pub role_counts: BTreeMap<String, usize>,
    /// Terms missing an explicit `gmeow:graphBoxRole`.
    pub missing: Vec<RoleFinding>,
    /// Terms whose role value is not a typed `gmeow:GraphBoxRole`.
    pub invalid: Vec<RoleFinding>,
}

impl BoxRoleAudit {
    /// Whether the audit found complete, valid role coverage.
    pub fn ok(&self) -> bool {
        self.missing.is_empty() && self.invalid.is_empty()
    }
}

/// The GMEOW term-kind priority order (mirrors `box_roles._KIND_ORDER`).
const KIND_INDIVIDUAL: &str = "individual";

/// Mirror of `box_roles._is_gmeow_term`.
fn is_gmeow_term(iri: &str, ontology_iri: &str, namespace: &str) -> bool {
    iri == ontology_iri || iri.starts_with(namespace)
}

/// Mirror of `box_roles._term_kind`: assign ONE kind by priority.
fn term_kind(types: &BTreeSet<String>) -> &'static str {
    if types.contains(owl::ONTOLOGY) {
        "ontology"
    } else if types.contains(owl::CLASS) {
        "class"
    } else if types.contains(owl::ANNOTATION_PROPERTY) {
        "annotation property"
    } else if types.contains(owl::OBJECT_PROPERTY) || types.contains(owl::DATATYPE_PROPERTY) {
        "property"
    } else if types.contains(rdfs::DATATYPE) {
        "datatype"
    } else {
        KIND_INDIVIDUAL
    }
}

/// Mirror of `box_roles._curie`.
fn curie(iri: &str, ontology_iri: &str, namespace: &str) -> String {
    if iri == ontology_iri {
        "gmeow:".to_owned()
    } else if let Some(rest) = iri.strip_prefix(namespace) {
        format!("gmeow:{rest}")
    } else {
        iri.to_owned()
    }
}

/// Mirror of `box_roles._source`: path relative to cwd if possible, else absolute.
fn source_str(path: &Path) -> String {
    match std::env::current_dir() {
        Ok(cwd) => match path.strip_prefix(&cwd) {
            Ok(rel) => rel.display().to_string(),
            Err(_) => path.display().to_string(),
        },
        Err(_) => path.display().to_string(),
    }
}

/// Audit explicit `gmeow:graphBoxRole` coverage for typed GMEOW terms.
///
/// Mirrors `gmeow_tools.box_roles.audit_box_roles`. Each path is parsed
/// individually to attribute a SOURCE to every term (first file a term's
/// type-triple appears in, matching the Python `setdefault`); a merged dataset
/// backs the role lookups and the `gmeow:GraphBoxRole`-membership check.
///
/// # Errors
///
/// Fails if any source fails to read or parse.
pub fn audit_box_roles(
    paths: &[PathBuf],
    ontology_iri: &str,
    namespace: &str,
) -> gmeow_errors::Result<BoxRoleAudit> {
    let graph_box_role = format!("{namespace}graphBoxRole");
    let graph_box_role_class = format!("{namespace}GraphBoxRole");

    // Per-term source attribution (first file a term's type-triple appears in)
    // and the accumulated set of each term's rdf:type IRIs. Insertion into a
    // BTreeMap keeps the eventual term iteration in sorted-IRI order, matching
    // the Python `sorted(types_by_term.items())` walk.
    let mut source_by_term: BTreeMap<String, String> = BTreeMap::new();
    let mut types_by_term: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    let mut builder = RdfDatasetBuilder::new();
    for path in paths {
        let dataset = store::parse_file_dataset(path)?;
        if let Some(type_id) = dataset.term_id_by_value(&TermValue::iri(rdf::TYPE)) {
            for q in dataset.quads_for_pattern(None, Some(type_id), None, GraphMatch::Any) {
                let TermRef::Iri(term) = dataset.resolve(q.s) else {
                    continue;
                };
                let TermRef::Iri(rdf_type) = dataset.resolve(q.o) else {
                    continue;
                };
                if !is_gmeow_term(term, ontology_iri, namespace) {
                    continue;
                }
                source_by_term
                    .entry(term.to_owned())
                    .or_insert_with(|| source_str(path));
                types_by_term
                    .entry(term.to_owned())
                    .or_default()
                    .insert(rdf_type.to_owned());
            }
        }
        // Fold this file into the merged view under a fresh blank scope, reusing the
        // parse above instead of reading every file from disk a second time.
        builder.push_dataset(&dataset);
    }

    // The merged dataset (blank-scoped per file, deduped at freeze) backs the role
    // lookups and the role-typing check.
    let merged = builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Serialize {
            detail: format!("dataset freeze failed: {e}"),
        })
    })?;
    let role_pred_id = merged.term_id_by_value(&TermValue::iri(&graph_box_role));
    let role_class_id = merged.term_id_by_value(&TermValue::iri(&graph_box_role_class));
    let type_id = merged.term_id_by_value(&TermValue::iri(rdf::TYPE));

    let mut report = BoxRoleAudit {
        term_count: types_by_term.len(),
        ..BoxRoleAudit::default()
    };

    for (term, types) in &types_by_term {
        let kind = term_kind(types).to_owned();
        let source = source_by_term
            .get(term)
            .cloned()
            .unwrap_or_else(|| source_str(Path::new("")));

        let roles = collect_roles(&merged, term, role_pred_id);
        if roles.is_empty() {
            report.missing.push(RoleFinding {
                term: term.clone(),
                kind,
                source,
                message: "missing gmeow:graphBoxRole".to_owned(),
            });
            continue;
        }
        for role in roles {
            // A non-IRI role object: Python emits `non-IRI gmeow:graphBoxRole
            // value {role.n3()}`. IRI-only roles are the norm; `collect_roles`
            // already restricts to IRIs, so this branch is unreachable on the
            // production path and the non-IRI case is folded there.
            if !role_typed_as_graph_box_role(&merged, &role, type_id, role_class_id) {
                report.invalid.push(RoleFinding {
                    term: term.clone(),
                    kind: kind.clone(),
                    source: source.clone(),
                    message: format!("{role} is not typed gmeow:GraphBoxRole"),
                });
                continue;
            }
            *report
                .role_counts
                .entry(curie(&role, ontology_iri, namespace))
                .or_insert(0) += 1;
        }
    }

    Ok(report)
}

/// Collect the IRI objects of `(term, gmeow:graphBoxRole, ?)` in the merged graph.
fn collect_roles(
    merged: &purrdf::RdfDataset,
    term: &str,
    role_pred_id: Option<purrdf::TermId>,
) -> Vec<String> {
    let (Some(pred_id), Some(term_id)) =
        (role_pred_id, merged.term_id_by_value(&TermValue::iri(term)))
    else {
        return Vec::new();
    };
    let mut roles = Vec::new();
    for q in merged.quads_for_pattern(Some(term_id), Some(pred_id), None, GraphMatch::Any) {
        if let TermRef::Iri(role) = merged.resolve(q.o) {
            roles.push(role.to_owned());
        }
    }
    roles
}

/// Whether `(role, rdf:type, gmeow:GraphBoxRole)` is in the merged graph.
fn role_typed_as_graph_box_role(
    merged: &purrdf::RdfDataset,
    role: &str,
    type_id: Option<purrdf::TermId>,
    role_class_id: Option<purrdf::TermId>,
) -> bool {
    let (Some(type_id), Some(role_class_id), Some(role_id)) = (
        type_id,
        role_class_id,
        merged.term_id_by_value(&TermValue::iri(role)),
    ) else {
        return false;
    };
    merged
        .quads_for_pattern(
            Some(role_id),
            Some(type_id),
            Some(role_class_id),
            GraphMatch::Any,
        )
        .next()
        .is_some()
}

/// Render the concise human-facing audit report (mirrors `box_roles.render_text`).
pub fn render_text(report: &BoxRoleAudit, ontology_iri: &str, namespace: &str) -> String {
    let mut lines: Vec<String> = vec![
        format!("Typed GMEOW terms: {}", report.term_count),
        "Role distribution:".to_owned(),
    ];
    if report.role_counts.is_empty() {
        lines.push("  none".to_owned());
    } else {
        for (role, count) in &report.role_counts {
            lines.push(format!("  {role}: {count}"));
        }
    }
    if !report.missing.is_empty() {
        lines.push(String::new());
        lines.push(format!("Missing roles ({}):", report.missing.len()));
        lines.extend(finding_lines(&report.missing, ontology_iri, namespace));
    }
    if !report.invalid.is_empty() {
        lines.push(String::new());
        lines.push(format!("Invalid roles ({}):", report.invalid.len()));
        lines.extend(finding_lines(&report.invalid, ontology_iri, namespace));
    }
    if report.ok() {
        lines.push(String::new());
        lines.push("All typed GMEOW terms have explicit typed graph-box roles.".to_owned());
    }
    lines.join("\n")
}

/// Mirror of `box_roles._finding_lines` (with the same `limit = 50`).
fn finding_lines(findings: &[RoleFinding], ontology_iri: &str, namespace: &str) -> Vec<String> {
    const LIMIT: usize = 50;
    let mut lines: Vec<String> = findings
        .iter()
        .take(LIMIT)
        .map(|f| {
            format!(
                "  {} ({}, {}): {}",
                curie(&f.term, ontology_iri, namespace),
                f.kind,
                f.source,
                f.message
            )
        })
        .collect();
    if findings.len() > LIMIT {
        lines.push(format!("  ... {} more", findings.len() - LIMIT));
    }
    lines
}

/// Render the audit as stable JSON (mirrors `box_roles.render_json`):
/// `json.dumps(as_dict, indent=2, sort_keys=True)`.
pub fn render_json(report: &BoxRoleAudit) -> String {
    let value = serde_json::json!({
        "ok": report.ok(),
        "termCount": report.term_count,
        // `role_counts` is a BTreeMap, so serde emits a sorted-key JSON object directly.
        "roleCounts": report.role_counts,
        "missing": report.missing.iter().map(finding_json).collect::<Vec<_>>(),
        "invalid": report.invalid.iter().map(finding_json).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&value).expect("serde_json::Value serializes infallibly")
}

/// Mirror of `box_roles._finding_dict`.
fn finding_json(finding: &RoleFinding) -> serde_json::Value {
    serde_json::json!({
        "term": finding.term,
        "kind": finding.kind,
        "source": finding.source,
        "message": finding.message,
    })
}

/// Project a box-role audit into the canonical diagnostics report.
///
/// Mirrors `box_roles.to_diagnostics_report`: both missing and invalid coverage
/// are gate-failing, so every finding is an `error`; the term's source rides in
/// the finding `path`, the term kind in `tags`, and the message is
/// `"{curie(term)} ({kind}): {message}"`.
pub fn to_diagnostics_report(report: &BoxRoleAudit, ontology_iri: &str, namespace: &str) -> Report {
    const TOOL: &str = "box-roles";
    let item = |finding: &RoleFinding, code: &str| -> Finding {
        let message = format!(
            "{} ({}): {}",
            curie(&finding.term, ontology_iri, namespace),
            finding.kind,
            finding.message
        );
        let mut f = Finding::new(Severity::Error, code, message).with_tool(TOOL);
        f.tags = vec![finding.kind.clone()];
        f.add_location(Location::new(
            Some(finding.source.clone()),
            None,
            None,
            None,
        ));
        f
    };

    let mut out = Report::new(TOOL);
    for finding in &report.missing {
        out.add_finding(item(finding, crate::codes::BOX_ROLES_MISSING));
    }
    for finding in &report.invalid {
        out.add_finding(item(finding, crate::codes::BOX_ROLES_INVALID));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    /// Write `contents` to `name` inside a fresh RAII temp directory.
    ///
    /// The returned [`tempfile::TempDir`] owns the directory: it is removed on
    /// drop, including on panic and early return. Bind it to a named `_tmp`
    /// (never a bare `_`, which would drop it immediately) so it outlives the
    /// path. The file *name* is preserved because the audit dispatches on the
    /// `.ttl` extension and on file stems.
    fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[test]
    fn box_role_audit_passes_for_explicit_typed_role() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_box_roles_pass.ttl",
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             ex:tbox a gmeow:GraphBoxRole .\n\
             gmeow:Documented\n\
                 a owl:Class ;\n\
                 gmeow:graphBoxRole ex:tbox .\n",
        );
        let report = audit_box_roles(std::slice::from_ref(&path), ONTOLOGY_IRI, NS).unwrap();
        assert!(report.ok());
        assert_eq!(report.role_counts.get("https://example.org/tbox"), Some(&1));
    }

    #[test]
    fn box_role_audit_reports_missing_and_invalid_roles() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_box_roles_missing_invalid.ttl",
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:MissingRole a owl:Class .\n\
             gmeow:InvalidRole\n\
                 a owl:Class ;\n\
                 gmeow:graphBoxRole ex:notTypedAsRole .\n",
        );
        let report = audit_box_roles(std::slice::from_ref(&path), ONTOLOGY_IRI, NS).unwrap();
        assert!(!report.ok());
        assert_eq!(
            report
                .missing
                .iter()
                .map(|f| f.term.clone())
                .collect::<Vec<_>>(),
            vec!["https://blackcatinformatics.ca/gmeow/MissingRole".to_owned()]
        );
        assert_eq!(
            report
                .invalid
                .iter()
                .map(|f| f.term.clone())
                .collect::<Vec<_>>(),
            vec!["https://blackcatinformatics.ca/gmeow/InvalidRole".to_owned()]
        );
        let text = render_text(&report, ONTOLOGY_IRI, NS);
        assert!(text.contains("Missing roles (1)"), "text was: {text}");
        assert!(text.contains("Invalid roles (1)"), "text was: {text}");
    }

    #[test]
    fn to_diagnostics_report_maps_missing_and_invalid() {
        let (_tmp, path) = write_tmp(
            "gmeow_validate_box_roles_diag.ttl",
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:MissingRole a owl:Class .\n\
             gmeow:InvalidRole\n\
                 a owl:Class ;\n\
                 gmeow:graphBoxRole ex:notTypedAsRole .\n",
        );
        let audit = audit_box_roles(std::slice::from_ref(&path), ONTOLOGY_IRI, NS).unwrap();
        let report = to_diagnostics_report(&audit, ONTOLOGY_IRI, NS);

        assert_eq!(report.tool, "box-roles");
        assert_eq!(report.error_count(), 2);
        assert_eq!(report.warning_count(), 0);
        let codes: BTreeSet<String> = report.findings.iter().map(|f| f.code.clone()).collect();
        let expected: BTreeSet<String> = [
            "box-roles.missing".to_owned(),
            "box-roles.invalid".to_owned(),
        ]
        .into_iter()
        .collect();
        assert_eq!(codes, expected);
    }

    #[test]
    fn to_diagnostics_report_clean_audit_is_ok() {
        let audit = audit_box_roles(&[], ONTOLOGY_IRI, NS).unwrap();
        let report = to_diagnostics_report(&audit, ONTOLOGY_IRI, NS);
        assert!(report.ok());
        assert_eq!(report.findings.len(), 0);
    }

    #[test]
    fn box_role_audit_with_empty_paths_audits_nothing() {
        let report = audit_box_roles(&[], ONTOLOGY_IRI, NS).unwrap();
        assert!(report.ok());
        assert_eq!(report.term_count, 0);
        assert!(report.role_counts.is_empty());
        assert!(report.missing.is_empty());
        assert!(report.invalid.is_empty());
        let text = render_text(&report, ONTOLOGY_IRI, NS);
        assert!(text.contains("Typed GMEOW terms: 0"), "text was: {text}");
        assert!(
            text.contains("All typed GMEOW terms have explicit typed graph-box roles."),
            "text was: {text}"
        );
    }
}
