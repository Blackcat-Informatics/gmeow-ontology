// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed observations attached to the source documentation model by its producers.

use super::{DocTerm, DocTermCategory};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The native reasoner's consistency verdict, attached to a [`super::DocsModel`] by the
/// production build so the docs can surface a per-term reasoning-status badge.
///
/// The native axes and unsupported observations remain explicit. Proved class
/// emptiness survives an undecided global result; other classes cannot acquire
/// a satisfiability claim from the absence of a counterexample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningVerdict {
    /// Documentation projection of the validated native consistency evidence.
    pub consistency: ConsistencyAssessment,
    /// Exact native input-admission token.
    pub input: String,
    /// Exact native evaluation token, including unsupported or budget exhaustion.
    pub evaluation: String,
    /// Exact native completeness token.
    pub completeness: String,
    /// Exact native information-state token.
    pub information: String,
    /// Complete native unsupported observations, never discarded to render docs.
    pub unsupported_constructs: BTreeSet<String>,
    /// The class IRIs proven unsatisfiable (entailed `rdfs:subClassOf owl:Nothing`).
    /// A non-empty set lights the affected class pages red without deciding
    /// the realizability of the remaining classes.
    pub unsatisfiable: std::collections::BTreeSet<String>,
}

/// A documentation assessment is an observation, never repository gate approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConsistencyAssessment {
    /// A complete admitted native result establishes consistency.
    Consistent,
    /// Validated positive contradiction evidence establishes inconsistency.
    Inconsistent,
    /// The native result does not decide global consistency.
    Undetermined,
}

impl ReasoningVerdict {
    /// Shared status projection for term pages, RDF and badges. Global consistency
    /// does not prove individual class realizability; only proved emptiness is known.
    #[must_use]
    pub fn term_status(&self, term: &DocTerm) -> &'static str {
        if term.category != DocTermCategory::Class {
            "not-evaluated"
        } else if self.unsatisfiable.contains(&term.iri) {
            "unsatisfiable"
        } else {
            "undetermined"
        }
    }
}

/// One display row projected from a complete native `gmeow_errors::Finding`.
/// The report retains all source evidence and standpoint fields; this docs row
/// carries the visible severity/category/message, first attributed slice, and an
/// exact constraint-code help link when that code exists in the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocDiagFinding {
    /// The stable diagnostic code (`Finding::code`, e.g. `shacl.MinCountConstraintComponent`
    /// or `logic-compile.UNKNOWN_PROFILE`).
    pub code: String,
    /// The display spelling of `Finding::severity` (`Severity::as_str()`).
    pub severity: String,
    /// The display spelling of `Finding::category` (`FindingCategory::as_str()`).
    pub category: String,
    /// The finding's complete human-readable message.
    pub message: String,
    /// The primary (first) attribution's slice IRI, when the finding carries one.
    pub slice_iri: Option<String>,
    /// The constraint-catalog rule's absolute help URI, resolved by exact `code`
    /// match against `DocsModel::constraint_rules`. `None` when no rule shares this
    /// exact code (an honest absence — never a fabricated deep link).
    pub help_uri: Option<String>,
}

/// Exact term and slice joins over the compile and validation producers' final
/// native Reports, or the same report Arcs retained by their snapshot. Primary
/// term attribution uses every `Finding::documented_terms` entry that names a
/// known DocTerm. The first logical location supplies a secondary exact match
/// only when not already documented. Slice joins use every supplied attribution;
/// no missing term, slice or help link is fabricated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiagnosticsDigest {
    /// Findings keyed by the exact documented or logical term IRI (sorted keys; each
    /// finding list is in stable normalized report order).
    pub by_term: BTreeMap<String, Vec<DocDiagFinding>>,
    /// Findings keyed by every attributed slice IRI (sorted keys; each finding list
    /// is in stable normalized report order).
    pub by_slice: BTreeMap<String, Vec<DocDiagFinding>>,
    /// The total number of findings folded from both upstream products
    /// (before any term/slice join — the raw union count).
    pub total: usize,
}

/// One row of the per-term dynamic projection-loss join: a single
/// `logic:ProjectionTarget` from the live `GRAPH_PROJECTION_LEDGER` graph whose
/// `rdfs:label` carries the `property-path:<shape-iri>` prefix and resolved to a
/// documented term (see [`TermLossDigest`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TermLossRow {
    /// The FULL `rdfs:label` value carried by the `logic:ProjectionTarget`
    /// (e.g. `property-path:https://.../nearbyOrgs`), kept whole (not stripped
    /// of its prefix) so the row is traceable back to its exact ledger entry.
    pub target: String,
    /// The local name of the `logic:preservationKind` object IRI (e.g.
    /// `SoundUnderApproximation`, `ExactPreservation`).
    pub preservation_kind: String,
    /// The `logic:complexityClass` plain-string literal.
    pub complexity_class: String,
    /// The `gmeow:lossyDrop` plain-string literals, sorted and deduped.
    pub lossy_drops: Vec<String>,
}

/// The dynamic per-term projection-loss join, folded from the LIVE
/// `GRAPH_PROJECTION_LEDGER` named graph — `stage-mappings`'s committed
/// projection report, attached AFTER source discovery (never a re-run of the
/// logic compiler; reason/compile-once). DISTINCT from the STATIC whole-program
/// rows already rendered on `Page::LogicLossLedger`
/// (`gmeow_logic_compile::projections::projection_ledger_rows`, e.g. `owl-dl`,
/// `datalog`) and from the authored worked examples
/// ([`super::DocsModel::loss_targets`], A4): this digest carries ONLY the per-shape
/// `property-path:<shape-iri>` rows the ledger emits when a concrete
/// `logic:PathShape` is compiled, joined to a documented term via
/// [`super::DocShape::shape_iri`] → [`super::DocShape::target_term`] (falling back to an
/// exact match of the bare shape IRI against a [`DocTerm::iri`] when no
/// `DocShape` claims it). Whole-program rows never enter `by_term` — they
/// apply project-wide, not per-term, and stay on the static ledger page. A
/// property-path row that resolves to no documented term is honestly absent
/// from `by_term` (never forced) — see [`total_property_path_rows`](
/// Self::total_property_path_rows) for the raw pre-join count, so a real-repo
/// non-vacuity assertion can distinguish "the ledger genuinely has no
/// property-path content" from "content exists but nothing joined a term".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TermLossDigest {
    /// Rows keyed by the exact documented term IRI they joined to (sorted keys;
    /// each row list sorted by `target`).
    pub by_term: BTreeMap<String, Vec<TermLossRow>>,
    /// The total count of `property-path:`-prefixed `logic:ProjectionTarget`
    /// rows found in the live ledger, whether or not they joined to a
    /// documented term.
    pub total_property_path_rows: usize,
}

/// The per-term JSON Schema / OpenAPI fragment join for the term-page
/// "use this term without RDF" panel + OpenAPI tab.
///
/// Both maps are keyed by the exact documented term IRI; each value is the
/// pretty-printed, deterministic JSON text of that class's `$defs` (respectively
/// `components/schemas`) fragment, extracted from the generated
/// `gmeow.schema.json` / `gmeow.openapi.json`. Only documented classes whose
/// emitter def key (`purrdf::shapes::json_schema::Namespaces::def_key`: a bare
/// local name for a primary-namespace class, a CURIE otherwise) resolves an
/// entry appear — a term with no schema fragment is honestly absent, never a
/// fabricated stub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SchemaFragmentDigest {
    /// The JSON Schema `$defs` fragment text keyed by documented term IRI.
    pub schema_by_term: BTreeMap<String, String>,
    /// The OpenAPI `components/schemas` fragment text keyed by documented term IRI.
    pub openapi_by_term: BTreeMap<String, String>,
}
