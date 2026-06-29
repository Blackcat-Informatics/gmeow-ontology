// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Repo-free Tier-1 conformance of an external RDF data file against the bundled
//! ontology's SHACL shapes and OntoUML disciplines.
//!
//! Where [`crate::validate_all`] is the slice-authoring dev gate (structural and
//! naming lint, example coverage, DSL phases) run over the repository sources,
//! this is the *consumer* path: it takes an arbitrary RDF data graph plus a
//! `gmeow.gts` bundle and runs only the two Tier-1 engines a downstream user
//! cares about —
//!
//! 1. **SHACL** against the data-graph shape union carried in the bundle's
//!    `shapes-archive` blob (every committed `shapes/*.ttl` and
//!    `generated/shapes/*.ttl` plus every per-slice `shapes.ttl`, minus the four
//!    DSL/manifest lint shapes that only target authoring sources, not the data
//!    graph); and
//! 2. the six **gUFO/OntoUML disciplines** ([`crate::gufo::reasoning_invariants`]).
//!
//! Tier-1 runs no reasoner. The opt-in **Tier-2 `--deep`** pass additionally runs
//! the native DL reasoner over the user's data graph MERGED with the bundle's
//! axioms, surfacing entailed contradictions the structural checks cannot see; it
//! degrades gracefully (an advisory note, never a hard failure) if the semantic
//! pass cannot run. The bundle is the only input besides the data file, so the path
//! is repo-free and Docker-free: an installed wheel carrying the folded `gmeow.gts`
//! is sufficient.
//!
//! The data-graph shape *selection* is authoritative here in Rust (the bundle
//! reader untars `shapes-archive` and applies the exclusion set) rather than in
//! the Python CLI surface, which passes only raw bytes.

use std::sync::Arc;

use gmeow_diagnostics::model::Location;
use gmeow_diagnostics::{Finding, Report, Severity};
use gmeow_rdf::RdfDataset;
use gmeow_shacl::shape_union::EXCLUDED;
use oxigraph::store::Store;

use crate::gufo::{self, GufoConfig};
use crate::store;
use crate::validate_all::{build_report, shacl_findings_from_report};

/// The blob `rep` label under which the snapshot stage folds the full SHACL shape
/// surface (`shapes-archive`). MUST match the writer in the pipeline snapshot
/// stage and the Python `bundle` reader.
const REP_SHAPES: &str = "shapes-archive";

/// Run Tier-1 conformance of `data_bytes` (an RDF graph in `data_format`) against
/// the shapes and disciplines carried in `gts_bytes`.
///
/// `data_format` is a media type or short format id understood by
/// [`gmeow_rdf::parse_dataset`] (`turtle`/`ttl`, `trig`, `n-triples`/`nt`,
/// `n-quads`/`nq`, `rdf+xml`) or the JSON-LD ids `json-ld`/`jsonld`. `namespace`
/// is the GMEOW IRI prefix the discipline checks key on. `origin` is the data
/// file's display path, recorded as each SHACL finding's physical location so
/// SARIF `artifactLocation.uri` points at the user's file.
///
/// Tier-1 validates the data graph in isolation (no ontology merge): every shape is
/// self-contained (`sh:targetClass` + constraints), so direct `rdf:type`
/// assertions resolve without the TBox, and the finding set reflects only the
/// user's graph. Named graphs in TriG/N-Quads are flattened to the default graph
/// so the shapes see every triple.
///
/// When `deep` is set, the opt-in **Tier-2** semantic pass additionally reasons over
/// the user's data MERGED with the bundle's axioms and folds the shared
/// `logic:ReasoningResult` verdict into the same report. Tier-2 degrades gracefully:
/// any failure of the semantic pass becomes a single `validate.deep.unavailable`
/// advisory note, leaving the complete Tier-1 result and its exit code intact.
///
/// # Errors
///
/// Returns `Err` if the bundle carries no `shapes-archive` blob, the archive is
/// malformed, the shapes fail to parse, or the data graph fails to parse. A Tier-2
/// (`deep`) failure is NOT an error — it is folded as an advisory note.
pub fn run(
    data_bytes: &[u8],
    data_format: &str,
    gts_bytes: &[u8],
    namespace: &str,
    origin: &str,
    deep: bool,
) -> Result<Report, String> {
    let shapes_ttl = data_graph_shapes_from_gts(gts_bytes)?;
    let store = data_store(data_bytes, data_format)?;

    let shapes = gmeow_shacl::engine::parse_shapes(&shapes_ttl)
        .map_err(|e| format!("bundled SHACL shapes failed to parse: {e}"))?;
    let shacl_report = gmeow_shacl::engine::validate(&store, &shapes);
    let shacl_findings = shacl_findings_from_report(&shacl_report, Some(origin));

    let cfg = GufoConfig {
        namespace: namespace.to_owned(),
    };
    let discipline_findings = gufo::reasoning_findings(&store, &cfg);

    let mut report = build_report(Vec::new(), Vec::new(), shacl_findings);
    for mut f in discipline_findings {
        if let Some(loc) = f.locations.first_mut() {
            loc.path = Some(origin.to_owned());
        } else {
            f.add_location(Location {
                path: Some(origin.to_owned()),
                ..Location::default()
            });
        }
        report.add_finding(f);
    }

    // Tier-2 (`--deep`): opt-in native semantic pass over user data + bundle axioms.
    if deep {
        run_deep_pass(gts_bytes, data_bytes, data_format, origin, &mut report);
    }

    Ok(report)
}

/// Run the opt-in Tier-2 deep pass, folding either its verdict findings or — on any
/// failure — a single `validate.deep.unavailable` advisory note into `report`.
///
/// This is the graceful-degradation boundary: a Tier-2 failure NEVER propagates, so
/// the complete Tier-1 result and its exit code are always preserved. (In the shipped
/// consumer wheel the reasoner co-ships with the validator in one native extension,
/// so a literally absent reasoner is not a runtime state; this branch covers
/// reasoning parse/read/run failures, which equally satisfy the never-crash contract.)
fn run_deep_pass(
    gts_bytes: &[u8],
    data_bytes: &[u8],
    data_format: &str,
    origin: &str,
    report: &mut Report,
) {
    let start = report.findings.len();
    if let Err(e) = deep_consistency_findings(gts_bytes, data_bytes, data_format, report) {
        let mut finding = Finding::new(
            Severity::Note,
            "validate.deep.unavailable",
            format!("deep semantic pass skipped: {e}"),
        )
        .with_tool("validate");
        finding.add_location(Location {
            path: Some(origin.to_owned()),
            ..Location::default()
        });
        report.add_finding(finding);
        return;
    }
    for finding in &mut report.findings[start..] {
        if finding.locations.is_empty() {
            finding.add_location(Location {
                path: Some(origin.to_owned()),
                ..Location::default()
            });
        }
    }
}

/// The opt-in Tier-2 semantic pass: reason over the user's data graph merged with
/// the bundle's axioms and fold the shared `logic:ReasoningResult` verdict into
/// `report` via [`crate::validate_all::fold_reasoning_result`] (the single fold the
/// dev bundle-only pass also uses).
///
/// Unlike Tier-1 (which flattens to the default graph for SHACL), the reasoning
/// dataset is parsed graph-preserving so the world-scoped native reasoner sees the
/// user's worlds. This is a second parse of the data bytes, paid only on `--deep`.
///
/// # Errors
///
/// Returns `Err` if the bundle cannot be read, the user data cannot be parsed into a
/// reasoning dataset, or the native reasoning run fails. The caller turns any such
/// error into an advisory note (graceful degradation).
fn deep_consistency_findings(
    gts_bytes: &[u8],
    data_bytes: &[u8],
    data_format: &str,
    report: &mut Report,
) -> Result<(), String> {
    let bundle =
        gmeow_rdf::import_gts_events(gts_bytes).map_err(|e| format!("GTS read error: {e}"))?;
    let user = data_dataset(data_bytes, data_format)?;
    let result = gmeow_logic::reason::reason_all_with_data(bundle.dataset.as_ref(), user.as_ref())
        .map_err(|e| format!("native reasoning failed: {e}"))?;
    // The governing contradiction policy is READ from the bundle's declared
    // logic:ReasoningContract (logic:admissibleValuation), not pinned: no contract /
    // no valuation ⇒ conservative classical DEFAULT (a glut IS owl:Nothing); multiple
    // conflicting valuations ⇒ the MOST CONSERVATIVE governs; a garbled valuation
    // HARD-FAILS rather than silently relaxing the gate. The policy is read off the
    // bundle (the authority for the contract), not the user-supplied data graph.
    let policy = gmeow_logic::certificate::ContradictionPolicy::resolve_from_dataset(
        bundle.dataset.as_ref(),
    )
    .map_err(|e| format!("contract resolution failed: {e}"))?;
    crate::validate_all::fold_reasoning_result(&result, policy, report);
    Ok(())
}

/// Parse external RDF data bytes into a graph-preserving [`RdfDataset`] for the
/// Tier-2 reasoner (the world structure must survive, so this does NOT flatten the
/// way [`data_store`] does for SHACL). Handles every supported format, routing
/// JSON-LD through the gmeow-gts codec exactly as [`data_store`] does.
fn data_dataset(data_bytes: &[u8], data_format: &str) -> Result<Arc<RdfDataset>, String> {
    if is_json_ld(data_format) {
        let text = std::str::from_utf8(data_bytes)
            .map_err(|e| format!("data file is not valid UTF-8: {e}"))?;
        let gts = gmeow_gts::from_yamlld::from_json_ld(text)
            .map_err(|e| format!("JSON-LD parse error: {e}"))?;
        let bundle = gmeow_rdf::import_gts_events(&gts)
            .map_err(|e| format!("JSON-LD dataset read error: {e}"))?;
        return Ok(bundle.dataset);
    }
    gmeow_rdf::parse_dataset(data_bytes, data_format, None)
        .map_err(|e| format!("data graph parse error: {e}"))
}

/// Build an in-memory oxigraph store from external RDF data bytes, flattening any
/// named graphs into the default graph so the shapes and discipline checks see the
/// whole graph.
fn data_store(data_bytes: &[u8], data_format: &str) -> Result<Store, String> {
    use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};

    if is_json_ld(data_format) {
        // JSON-LD has no native-codec media type; route it through the gmeow-gts
        // JSON-LD codec to GTS bytes, then fold to a flattened store.
        let text = std::str::from_utf8(data_bytes)
            .map_err(|e| format!("data file is not valid UTF-8: {e}"))?;
        let gts = gmeow_gts::from_yamlld::from_json_ld(text)
            .map_err(|e| format!("JSON-LD parse error: {e}"))?;
        let graph = store::read_gts_graph(&gts)?;
        return store::build_store_from_graph(&graph);
    }

    let dataset = gmeow_rdf::parse_dataset(data_bytes, data_format, None)
        .map_err(|e| format!("data graph parse error: {e}"))?;
    store_from_dataset(&dataset, GraphPolicy::FlattenToDefaultGraph).map_err(|e| e.to_string())
}

/// True for the JSON-LD format ids (handled outside the native-codec router).
fn is_json_ld(format: &str) -> bool {
    let f = format.trim().to_ascii_lowercase();
    matches!(
        f.as_str(),
        "json-ld" | "jsonld" | "application/ld+json" | "ld+json"
    )
}

/// Extract and assemble the data-graph SHACL shape union (one Turtle document)
/// from the bundle's `shapes-archive` blob.
fn data_graph_shapes_from_gts(gts_bytes: &[u8]) -> Result<String, String> {
    let mut graph = store::read_gts_graph(gts_bytes)?;

    // Resolve the digest of the blob declared with rep == "shapes-archive".
    // `blob_meta` values are CBOR maps (`ciborium::value::Value::Map`); read the
    // `rep` text field rather than indexing a JSON object.
    let digest = graph
        .blob_meta
        .iter()
        .find(|(_, meta)| cbor_text_field(meta, "rep") == Some(REP_SHAPES))
        .map(|(d, _)| d.clone())
        .ok_or_else(|| {
            format!("bundle carries no `{REP_SHAPES}` blob — cannot validate repo-free")
        })?;

    // Decode the blob bytes (forcing a lazy entry if the fold deferred it).
    let entry = graph
        .blobs
        .iter_mut()
        .find(|(d, _)| *d == digest)
        .map(|(_, e)| e)
        .ok_or_else(|| format!("`{REP_SHAPES}` blob metadata present but bytes missing"))?;
    let tar = entry
        .decode()
        .map_err(|e| format!("`{REP_SHAPES}` blob decode error: {e}"))?
        .to_vec();

    let mut members = gmeow_rdf::ustar::read_archive(&tar)?;
    // Deterministic concatenation order regardless of archive member order.
    members.sort_by(|a, b| a.0.cmp(&b.0));

    let mut ttl = String::new();
    let mut included = 0usize;
    for (name, bytes) in &members {
        if !name.ends_with(".ttl") {
            continue;
        }
        let base = name.rsplit('/').next().unwrap_or(name);
        if EXCLUDED.contains(&base) {
            continue;
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|e| format!("shape `{name}` is not valid UTF-8: {e}"))?;
        ttl.push_str(text);
        ttl.push('\n');
        included += 1;
    }

    if included == 0 {
        return Err(format!(
            "`{REP_SHAPES}` blob held no data-graph shapes — the bundle is incomplete"
        ));
    }
    Ok(ttl)
}

/// Read a text-valued field out of a CBOR map (`ciborium::value::Value::Map`),
/// matching the string key `key`. Returns `None` for a non-map value or a
/// missing/non-text field.
fn cbor_text_field<'a>(meta: &'a ciborium::value::Value, key: &str) -> Option<&'a str> {
    let ciborium::value::Value::Map(entries) = meta else {
        return None;
    };
    for (k, v) in entries {
        if let ciborium::value::Value::Text(name) = k {
            if name == key {
                if let ciborium::value::Value::Text(text) = v {
                    return Some(text.as_str());
                }
                return None;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_json_ld_matches_ids_and_media_type() {
        assert!(is_json_ld("json-ld"));
        assert!(is_json_ld("jsonld"));
        assert!(is_json_ld("application/ld+json"));
        assert!(is_json_ld("  JSON-LD  "));
        assert!(!is_json_ld("turtle"));
        assert!(!is_json_ld("application/json"));
    }

    #[test]
    fn deep_pass_failure_folds_advisory_note_and_preserves_tier1() {
        // Graceful degradation (AC2): when the Tier-2 pass cannot run — here the
        // bundle bytes are unreadable, so import_gts_events fails — the pre-existing
        // Tier-1 findings survive unchanged and exactly one validate.deep.unavailable
        // advisory Note is folded. No panic, no error propagation.
        let mut report = Report::new("validate");
        report.add_finding(
            Finding::new(
                Severity::Error,
                "tier1.fixture",
                "a pre-existing Tier-1 finding",
            )
            .with_tool("validate"),
        );

        run_deep_pass(
            b"not a gts bundle",
            b"ex:a ex:b ex:c .",
            "turtle",
            "fixture.ttl",
            &mut report,
        );

        let unavailable: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.code == "validate.deep.unavailable")
            .collect();
        assert_eq!(
            unavailable.len(),
            1,
            "exactly one advisory note on a failed deep pass: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
        assert_eq!(unavailable[0].severity, Severity::Note);
        assert_eq!(
            unavailable[0]
                .locations
                .first()
                .and_then(|l| l.path.as_deref()),
            Some("fixture.ttl"),
            "validate.deep.unavailable must carry the origin path as its location"
        );
        assert!(
            report.findings.iter().any(|f| f.code == "tier1.fixture"),
            "the pre-existing Tier-1 finding must be preserved"
        );
        // No inconsistency error was fabricated from the failed pass.
        assert!(!report
            .findings
            .iter()
            .any(|f| f.code == "validate.deep.inconsistent"));
    }

    #[test]
    fn cbor_text_field_reads_rep_label() {
        use ciborium::value::Value;
        let meta = Value::Map(vec![
            (
                Value::Text("mt".into()),
                Value::Text("application/x-tar".into()),
            ),
            (
                Value::Text("rep".into()),
                Value::Text("shapes-archive".into()),
            ),
        ]);
        assert_eq!(cbor_text_field(&meta, "rep"), Some("shapes-archive"));
        assert_eq!(cbor_text_field(&meta, "absent"), None);
        assert_eq!(cbor_text_field(&Value::Null, "rep"), None);
    }
}
