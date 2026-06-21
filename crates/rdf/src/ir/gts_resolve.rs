// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared GTS term resolver (#819 C2).
//!
//! The borrowing [`crate::gts::GtsGraphStore`] adapter and the consuming
//! [`super::import_graph`] importer both fold a *folded* `gmeow_gts::model::Graph`
//! into RDF terms, and both need the SAME depth-bounded structural traversal:
//! term-kind dispatch, the non-empty-IRI and datatype-must-be-IRI checks, reifier
//! lookup for quoted-triple terms, and the cyclic-nesting depth guard.
//!
//! This module is the single home of that traversal in its **eager** form:
//! [`term_from_id`] / [`triple_from_ids`] / [`predicate_from_id`] resolve a graph
//! term id into the [`RdfTerm`]/[`RdfTriple`] model by **cloning** the borrowed
//! graph's owned strings — the surface a `&Graph` view (`GtsGraphStore`) needs.
//!
//! The consuming `import_graph` importer cannot reuse these directly: it consumes a
//! `Graph` *by value* and MOVES term strings into the interner, which is structurally
//! incompatible with borrowing the same `Graph` for a clone-based resolver. It
//! therefore mirrors this traversal in move form, sharing the
//! [`MAX_GTS_TERM_NESTING_DEPTH`] bound and the `gts-*` diagnostic codes so the two
//! cannot drift on structural contract.
//!
//! The diagnostic codes here are the historical `gts-*` codes (preserved verbatim
//! from the original `gts.rs` implementation), so the existing `GtsGraphStore`
//! behavior — including its error contracts — is unchanged by the extraction.

use gmeow_gts::model::{Graph, TermKind};

use crate::{RdfDiagnostic, RdfLiteral, RdfLocation, RdfTerm, RdfTextDirection, RdfTriple};

/// Depth bound for resolving nested quoted-triple terms. A cyclic or absurdly
/// nested triple term hard-fails rather than recursing without bound. Shared by the
/// eager resolver here and the move-based importer in [`super::import_graph`].
pub(crate) const MAX_GTS_TERM_NESTING_DEPTH: usize = 16;

/// Parse a GTS literal base-direction string (`"ltr"`/`"rtl"`, gmeow-gts#212)
/// into the IR's [`RdfTextDirection`]. `None` is legitimate absence; an
/// unrecognized non-empty value is a hard error rather than a silent drop —
/// the GTS round-trip is ours, so a malformed direction is corrupt input, not
/// an intentional loss. Shared by all three decode paths (eager resolver,
/// consuming `import_graph`, streaming `import_sink`).
pub(crate) fn parse_gts_direction(
    value: Option<&str>,
) -> Result<Option<RdfTextDirection>, RdfDiagnostic> {
    match value {
        None => Ok(None),
        Some("ltr") => Ok(Some(RdfTextDirection::Ltr)),
        Some("rtl") => Ok(Some(RdfTextDirection::Rtl)),
        Some(other) => Err(RdfDiagnostic::error(
            "gts-invalid-direction",
            format!("unrecognized GTS literal base direction {other:?}"),
        )),
    }
}

/// Resolve a graph term id into an [`RdfTerm`], cloning the borrowed strings.
pub(crate) fn term_from_id(
    graph: &Graph,
    term_id: usize,
    location: RdfLocation,
) -> Result<RdfTerm, RdfDiagnostic> {
    term_from_id_depth(graph, term_id, location, 0)
}

/// Resolve a graph term id that MUST be an IRI into its string (predicate position).
pub(crate) fn predicate_from_id(
    graph: &Graph,
    term_id: usize,
    location: RdfLocation,
) -> Result<String, RdfDiagnostic> {
    predicate_from_id_depth(graph, term_id, location, 0)
}

/// Resolve a `(s, p, o)` triple of graph term ids into an [`RdfTriple`].
pub(crate) fn triple_from_ids(
    graph: &Graph,
    s: usize,
    p: usize,
    o: usize,
    location: RdfLocation,
) -> Result<RdfTriple, RdfDiagnostic> {
    triple_from_ids_depth(graph, s, p, o, location, 0)
}

fn triple_from_ids_depth(
    graph: &Graph,
    s: usize,
    p: usize,
    o: usize,
    location: RdfLocation,
    depth: usize,
) -> Result<RdfTriple, RdfDiagnostic> {
    let subject = term_from_id_depth(graph, s, location.clone(), depth)?;
    let predicate = predicate_from_id_depth(graph, p, location.clone(), depth)?;
    let object = term_from_id_depth(graph, o, location.clone(), depth)?;
    Ok(RdfTriple::new(subject, predicate, object).with_location(location))
}

fn predicate_from_id_depth(
    graph: &Graph,
    term_id: usize,
    location: RdfLocation,
    depth: usize,
) -> Result<String, RdfDiagnostic> {
    match term_from_id_depth(graph, term_id, location.clone(), depth)? {
        RdfTerm::Iri(iri) => Ok(iri),
        other => Err(RdfDiagnostic::error(
            "gts-predicate-not-iri",
            format!("GTS predicate term must be an IRI, got {:?}", other.kind()),
        )
        .with_location(location.with_gts_term(term_id))),
    }
}

fn term_from_id_depth(
    graph: &Graph,
    term_id: usize,
    location: RdfLocation,
    depth: usize,
) -> Result<RdfTerm, RdfDiagnostic> {
    if depth > MAX_GTS_TERM_NESTING_DEPTH {
        return Err(RdfDiagnostic::error(
            "gts-term-nesting-limit",
            "GTS term nesting depth limit exceeded",
        )
        .with_location(location.with_gts_term(term_id)));
    }
    let term = graph.terms.get(term_id).ok_or_else(|| {
        RdfDiagnostic::error(
            "gts-term-out-of-range",
            format!("GTS term id {term_id} is out of range"),
        )
        .with_location(location.clone().with_gts_term(term_id))
    })?;
    match term.kind {
        TermKind::Iri => {
            let Some(iri) = term.value.as_deref().filter(|value| !value.is_empty()) else {
                return Err(RdfDiagnostic::error(
                    "gts-iri-missing-value",
                    "GTS IRI term requires a non-empty value",
                )
                .with_location(location.with_gts_term(term_id)));
            };
            Ok(RdfTerm::iri(iri))
        }
        TermKind::Bnode => Ok(RdfTerm::blank_node(
            term.value
                .clone()
                .unwrap_or_else(|| format!("gts_bnode_{term_id}")),
        )),
        TermKind::Literal => {
            let datatype = match term.datatype {
                Some(datatype_id) => {
                    match term_from_id_depth(graph, datatype_id, location.clone(), depth + 1)? {
                        RdfTerm::Iri(iri) => Some(iri),
                        other => {
                            return Err(RdfDiagnostic::error(
                                "gts-literal-datatype-not-iri",
                                format!(
                                    "GTS literal datatype must resolve to an IRI, got {:?}",
                                    other.kind()
                                ),
                            )
                            .with_location(location.with_gts_term(datatype_id)));
                        }
                    }
                }
                None => None,
            };
            Ok(RdfTerm::literal(RdfLiteral {
                lexical_form: term.value.clone().unwrap_or_default(),
                datatype,
                language: term.lang.clone(),
                direction: parse_gts_direction(term.direction.as_deref())?,
            }))
        }
        TermKind::Triple => {
            let Some(reifier_id) = term.reifier else {
                return Err(RdfDiagnostic::error(
                    "gts-unbound-triple-term",
                    "GTS triple term has no reifier binding",
                )
                .with_location(location.with_gts_term(term_id)));
            };
            let Some((s, p, o)) = graph.reifier(reifier_id) else {
                return Err(RdfDiagnostic::error(
                    "gts-missing-reifier-binding",
                    format!("GTS triple term references missing reifier {reifier_id}"),
                )
                .with_location(location.with_gts_term(term_id).with_gts_reifier(reifier_id)));
            };
            Ok(RdfTerm::triple(triple_from_ids_depth(
                graph,
                s,
                p,
                o,
                location.with_gts_reifier(reifier_id),
                depth + 1,
            )?))
        }
    }
}
