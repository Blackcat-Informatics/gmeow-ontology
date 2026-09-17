// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free RDF ingestion for the validation lints (native `RdfDataset` IR).
//!
//! Every validation engine (coverage, lint, gUFO, statement, constitution, the DSL
//! SHACL merge, the data-graph path) reads a frozen [`purrdf::RdfDataset`]: the
//! sources are parsed once with the native purrdf codecs ([`parse_dataset`]),
//! merged under per-file blank scopes via [`purrdf::RdfDatasetBuilder`], and
//! queried through the indexed [`purrdf::DatasetView::quads_for_pattern`]. The
//! SHACL engine is itself native ([`shacl_validate_dataset`]).
//!
//! This module is fully oxigraph-free: every helper returns or queries the
//! native [`purrdf::RdfDataset`].
//!
//! Parsing is **lenient by construction**: the native codecs accept the GMEOW
//! ontology's private-use `@x-gmeow-*` language tags whose subtag exceeds BCP-47's
//! 8-char limit (e.g. `@x-gmeow-afrikaans`), while still surfacing every real Turtle
//! syntax error.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_errors::Diag;
use purrdf::{RdfDataset, RdfDatasetBuilder, parse_dataset};

use crate::model::{logic, owl};

/// Validate a native [`RdfDataset`] against parsed SHACL
/// [`purrdf::shapes::shapes::Shapes`] over the native
/// IR engine.
///
/// The SHACL engine is fully native (it takes an `RdfDataset` directly), so this is a
/// thin wrapper that surfaces the validation report and treats engine failure as
/// infallible for a frozen, validated dataset.
pub fn shacl_validate_dataset(
    dataset: &RdfDataset,
    shapes: &purrdf::shapes::shapes::Shapes,
) -> purrdf::shapes::report::ValidationReport {
    let mut report = purrdf::shapes::engine::validate_dataset(dataset, shapes)
        .expect("validation over a frozen dataset is infallible");
    dedupe_validation_results(&mut report);
    report
}

/// The total, order-preserving identity of a SHACL result: every field a consumer can
/// observe, joined under a field separator no IRI or message can contain.
///
/// Two results with equal identity are INDISTINGUISHABLE — same focus node, same path
/// (including the structure behind a complex-path blank node), same offending value,
/// same constraint component, same source shape, same severity, same message, same box
/// roles, same attributions. There is no observation that separates them.
fn result_identity(result: &purrdf::shapes::report::ValidationResult) -> String {
    use std::fmt::Write;
    let mut key = String::new();
    let field = |value: &dyn std::fmt::Debug, key: &mut String| {
        let _ = write!(key, "{value:?}\u{1f}");
    };
    field(&result.focus_node.to_string(), &mut key);
    field(
        &result.result_path.as_ref().map(ToString::to_string),
        &mut key,
    );
    field(&result.path_structure, &mut key);
    field(&result.value.as_ref().map(ToString::to_string), &mut key);
    field(&result.source_constraint_component.as_str(), &mut key);
    field(&result.source_shape.to_string(), &mut key);
    field(&result.severity, &mut key);
    field(&result.message, &mut key);
    field(&result.source_box_roles, &mut key);
    field(&result.path_box_roles, &mut key);
    field(&result.result_box_roles, &mut key);
    field(&result.attributions, &mut key);
    key
}

/// Collapse INDISTINGUISHABLE results in a SHACL report, keeping the first occurrence
/// of each (so the engine's own result order is preserved).
///
/// A SHACL validation report is a SET of results: a violation reported twice is one
/// violation, and a consumer that counts findings gets the wrong answer when it is
/// reported N times. The engine produces the duplicates honestly — SHACL specifies one
/// validation result per SOLUTION of an `sh:sparql` constraint's `sh:select`, and a
/// projected `SELECT $this` whose WHERE clause binds further variables (a guard triple
/// that fixes the target, an `?s1`/`?s2` pair witnessing a duplicate index) has one
/// solution per BINDING COMBINATION, not per focus node. Every such solution projects
/// the same single `$this` column, so the results it yields are byte-identical. The
/// same collapse handles equivalent execution paths that report the same shape
/// and finding. It does not repair an ill-formed shape: qualified minimum and
/// maximum bounds must share one qualified value shape, as the projector enforces.
///
/// Distinct violations always differ in at least one observable field and are never
/// collapsed — see [`result_identity`].
pub fn dedupe_validation_results(report: &mut purrdf::shapes::report::ValidationReport) {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    report
        .results
        .retain(|result| seen.insert(result_identity(result)));
}

/// Every indistinguishable-result group a SHACL report carries more than once: a human
/// description of the repeated result paired with how many times the engine reported
/// it, in first-appearance order.
///
/// [`dedupe_validation_results`] makes the shipped surfaces read the report as the SET
/// it is; this is the complementary AUDIT, for a gate that must FAIL on a duplicate
/// rather than quietly absorb it — a conformance cell that asserts "exactly one
/// finding" is only meaningful if something reds when the engine emits four.
#[must_use]
pub fn duplicate_validation_results(
    report: &purrdf::shapes::report::ValidationReport,
) -> Vec<(String, usize)> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: std::collections::HashMap<String, (String, usize)> =
        std::collections::HashMap::new();
    for result in &report.results {
        let identity = result_identity(result);
        match counts.get_mut(&identity) {
            Some((_, count)) => *count += 1,
            None => {
                order.push(identity.clone());
                counts.insert(
                    identity,
                    (
                        format!(
                            "{} at {} (shape {}{})",
                            result.source_constraint_component.as_str(),
                            result.focus_node,
                            result.source_shape,
                            result
                                .result_path
                                .as_ref()
                                .map(|p| format!(", path {p}"))
                                .unwrap_or_default(),
                        ),
                        1,
                    ),
                );
            }
        }
    }
    order
        .into_iter()
        .filter_map(|identity| counts.remove(&identity))
        .filter(|(_, count)| *count > 1)
        .collect()
}

/// Parse a single Turtle file into a frozen native [`RdfDataset`].
///
/// Lenient parsing (accepts GMEOW's private-use `@x-gmeow-*` language tags). The
/// returned dataset is uniquely owned (fresh off the native parser).
///
/// # Errors
///
/// Returns `Err` if the file cannot be read or the Turtle fails to parse.
pub fn parse_file_dataset(path: &Path) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let bytes = std::fs::read(path).map_err(|e| {
        Diag::of_kind(crate::error::Io {
            detail: e.to_string(),
        })
    })?;
    parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        Diag::of_kind(crate::error::Parse {
            detail: e.to_string(),
        })
    })
}

/// Build one merged frozen [`RdfDataset`] from every Turtle source in `paths`.
///
/// Each file is parsed under a fresh blank scope ([`RdfDatasetBuilder::push_dataset`])
/// so anonymous blanks across files stay disjoint (C0.2 — the native twin of the old
/// per-source blank-prefix scoping). Quads dedup at freeze (C0.5), matching the old
/// `Store::insert` set semantics. A malformed file aborts with an error naming the
/// file.
///
/// # Errors
///
/// Returns `Err` if any file fails to read or parse.
pub fn dataset_from_paths(paths: &[PathBuf]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for path in paths {
        let path_str = path.display().to_string();
        let bytes = std::fs::read(path).map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("failed to read {path_str}: {e}"),
            })
        })?;
        let dataset = parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
            Diag::of_kind(crate::error::Parse {
                detail: format!("syntax error in {path_str}: {e}"),
            })
        })?;
        builder.push_dataset(&dataset);
    }
    builder.freeze().map_err(|e| {
        Diag::of_kind(crate::error::Serialize {
            detail: format!("dataset freeze failed: {e}"),
        })
    })
}

/// Build a frozen native [`RdfDataset`] from an N-Triples document, flattening any
/// graph slot to the default graph (N-Triples is graphless).
///
/// Lenient parsing (private-use `@x-gmeow-*` language tags) — the data seam for the
/// rdflib-free validation path.
///
/// # Errors
///
/// Returns `Err` if the N-Triples fails to parse.
pub fn dataset_from_nt(data_nt: &str) -> gmeow_errors::Result<Arc<RdfDataset>> {
    parse_dataset(data_nt.as_bytes(), "application/n-triples", None).map_err(|e| {
        Diag::of_kind(crate::error::Parse {
            detail: format!("N-Triples parse error: {e}"),
        })
    })
}

/// Build a frozen native [`RdfDataset`] from a GTS byte bundle, flattening every named
/// graph into the default graph (so the lints/shapes see the whole graph).
///
/// The native streaming importer preserves segment blank-node scopes, reifiers and
/// annotations. The validation projection clears only graph slots after that import.
///
/// # Errors
/// Returns `Err` for malformed native input or an invalid flat projection.
pub fn dataset_from_gts(bytes: &[u8]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let imported = purrdf::import_gts_events(bytes).map_err(|error| {
        Diag::of_kind(crate::error::Dataset {
            detail: error.to_string(),
        })
    })?;
    crate::data_validate::flatten_to_default_graph(&imported.dataset)
}

/// Project a full `gmeow.gts` bundle into a **core browser bundle** — graph-preserving
/// N-Quads text carrying only the object-level ontology (the default graph) plus any
/// explicitly kept named graphs, with every derived/heavy graph dropped (the
/// documentation projection, the `graph/fanout/*` flat-file re-embeds, diagnostics,
/// authoring briefs, the reasoned closure, …).
///
/// The FULL bundle extracts to ~948 MB of N-Quads — far too large to load and query
/// in a browser (it OOMs the wasm engine). This projection keeps the queryable
/// object-level ontology (~124 k quads → ~24 MB N-Quads, well within a browser's
/// reach once the web server gzips it) so the in-browser playground/explorer can
/// parse and SPARQL over the SAME authored ontology the pipeline shipped. It is
/// shipped as N-Quads TEXT (not a GTS container) so the in-page purrdf RDF engine
/// parses it directly with no container codec, and it is a pure, deterministic
/// function of the input bytes (order-preserving filter + deterministic serializer),
/// so the emitted asset is byte-reproducible.
///
/// `keep_named_graphs` is the allow-list of named-graph IRIs to retain ALONGSIDE the
/// default graph (e.g. grounding graphs); pass an empty slice for object-level only.
///
/// # Errors
///
/// Returns `Err` if the container cannot be read, the statement layer cannot be
/// folded, or the filtered dataset cannot be serialized.
pub fn core_browser_bundle_nquads(
    full_bytes: &[u8],
    keep_named_graphs: &[&str],
) -> gmeow_errors::Result<String> {
    use std::collections::HashSet;
    let to_diag = |e: purrdf::RdfDiagnostic| {
        Diag::of_kind(crate::error::Dataset {
            detail: e.to_string(),
        })
    };
    let mut graph = purrdf::gts::read_all_segments(full_bytes).map_err(to_diag)?;
    // Term ids whose value is a kept named-graph IRI. The default graph (`None` slot)
    // is always retained; every other named graph is dropped.
    let keep: HashSet<usize> = graph
        .terms
        .iter()
        .enumerate()
        .filter_map(|(i, t)| match t.value.as_deref() {
            Some(v) if keep_named_graphs.contains(&v) => Some(i),
            _ => None,
        })
        .collect();
    let kept = |slot: Option<usize>| slot.is_none_or(|gid| keep.contains(&gid));
    graph.quads.retain(|q| kept(q.3));
    graph.reifiers.retain(|r| kept(r.2));
    graph.annotations.retain(|a| kept(a.3));
    // Fold the filtered graph into a graph-preserving dataset and serialize to
    // N-Quads over the full dataset selection (the term table rides in the codec, so
    // no term-pruning is needed — dropped quads simply do not appear).
    let dataset = purrdf::gts::dataset_from_gts_graph(&graph).map_err(to_diag)?;
    let bytes = purrdf::serialize_dataset(
        &*dataset,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(to_diag)?;
    String::from_utf8(bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: format!("core browser bundle N-Quads is not valid UTF-8: {e}"),
        })
    })
}

/// Read a `gmeow.gts` bundle's bytes into **graph-preserving** N-Quads text — every
/// base quad keeps its named-graph component (unlike [`dataset_from_gts`], which
/// folds them into the default graph). This is the browser bundle-read primitive:
/// the wasm shim (`gmeow-validate-wasm::bundle_dataset`) hands the resulting N-Quads
/// to the in-page purrdf RDF engine so the documentation playground/explorer query
/// the SAME bundle the pipeline shipped, rather than a second curated data path.
///
/// Uses the oxigraph-free container reader (`read_all_segments` →
/// `dataset_from_gts_graph`, which retains each quad's graph) and the native
/// N-Quads serializer over the full dataset selection; both are wasm-clean (no
/// reasoner, no filesystem).
///
/// # Errors
///
/// Returns `Err` if the GTS container cannot be read, the statement layer cannot be
/// folded, or the dataset cannot be serialized.
pub fn dataset_nquads_from_gts(bytes: &[u8]) -> gmeow_errors::Result<String> {
    let to_diag = |e: purrdf::RdfDiagnostic| {
        Diag::of_kind(crate::error::Dataset {
            detail: e.to_string(),
        })
    };
    let graph = purrdf::gts::read_all_segments(bytes).map_err(to_diag)?;
    let dataset = purrdf::gts::dataset_from_gts_graph(&graph).map_err(to_diag)?;
    let bytes = purrdf::serialize_dataset(
        &*dataset,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(to_diag)?;
    String::from_utf8(bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: format!("bundle N-Quads is not valid UTF-8: {e}"),
        })
    })
}

/// Render a resolved subject term the way the legacy `_ox_term_display` did:
/// IRI → its value; blank → `_:b`.
///
/// A triple subject is exactly an IRI or a blank node in well-formed RDF; a
/// literal/triple subject stringifies defensively (never reached on the validation
/// path).
pub fn subject_display(subject: purrdf::TermRef<'_>) -> String {
    use purrdf::TermRef;
    match subject {
        TermRef::Iri(iri) => iri.to_owned(),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        TermRef::Literal { lexical, .. } => lexical.to_owned(),
        TermRef::Triple { .. } => "<<triple>>".to_owned(),
    }
}

/// Scan a frozen native [`RdfDataset`] for Principle 5 `owl:sameAs`-to-external-entity
/// violations, in document (dataset) order.
///
/// A violation is every `owl:sameAs` triple whose object is an IRI that does NOT start
/// with `namespace`, unless `(subject_display, object)` is in `allowlist`. Returns the
/// `(subject_display, object)` pair for each violation — the caller frames the
/// user-facing message so the file path can be interpolated exactly as before.
pub fn sameas_violations(
    dataset: &RdfDataset,
    namespace: &str,
    allowlist: &[(String, String)],
) -> Vec<(String, String)> {
    use purrdf::{DatasetView, GraphMatch, TermRef, TermValue};

    // The Principle-5 ban scans identity in the canonical `logic:sameAs` spelling and
    // its generated `owl:sameAs` view — a slice authors `logic:sameAs` after the flip,
    // and an external-pointing identity in EITHER spelling is a violation. A dataset that
    // carries BOTH spellings of one identity (the authored `logic:sameAs` plus its
    // projected `owl:sameAs`) describes ONE violation, not two, so dedup the
    // `(subject, object)` pair across the two scans — first (canonical `logic:`) occurrence
    // wins, preserving document order. A consumer that counts findings must see one.
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for sameas_id in [logic::SAME_AS, owl::SAME_AS]
        .into_iter()
        .filter_map(|p| dataset.term_id_by_value(&TermValue::iri(p)))
    {
        for quad in dataset.quads_for_pattern(None, Some(sameas_id), None, GraphMatch::Any) {
            let TermRef::Iri(obj) = dataset.resolve(quad.o) else {
                continue;
            };
            if obj.starts_with(namespace) {
                continue;
            }
            let subject_text = subject_display(dataset.resolve(quad.s));
            if allowlist
                .iter()
                .any(|(s, o)| s == &subject_text && o == obj)
            {
                continue;
            }
            let pair = (subject_text, obj.to_owned());
            if seen.insert(pair.clone()) {
                out.push(pair);
            }
        }
    }
    out
}

/// Parse GTS bytes into a [`purrdf::gts::model::Graph`].
///
/// Folds the GTS bytes with all segments enabled (`allow_segments = true`). Any
/// non-empty diagnostic list is treated as a hard failure (fail-fast) so callers
/// never receive a silent partial graph from malformed or truncated GTS bytes.
///
/// # Errors
///
/// Returns `Err` if the GTS fold reports any diagnostics (corruption,
/// truncation, empty input, or unfolded segments).
pub fn read_gts_graph(bytes: &[u8]) -> gmeow_errors::Result<purrdf::gts::model::Graph> {
    purrdf::gts::read_all_segments(bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: e.to_string(),
        })
    })
}

#[path = "store.tests.rs"]
#[cfg(test)]
mod tests;
