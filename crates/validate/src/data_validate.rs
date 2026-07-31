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

use std::collections::BTreeSet;
use std::sync::Arc;

use gmeow_errors::Report;
use gmeow_errors::model::Location;
use purrdf::shapes::shape_union::EXCLUDED;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

use crate::gufo::{self, GufoConfig};
use crate::report_bridge::{build_report, shacl_findings_from_report};
use crate::store;

// `Finding`/`Severity` are only constructed by the native-only Tier-2 deep pass
// (and its tests). The wasm Tier-1 surface folds findings through `report_bridge`
// and never names these types directly.
#[cfg(not(target_arch = "wasm32"))]
use gmeow_errors::{Finding, Severity};

/// Typed error for the Tier-2 deep pass, distinguishing failure modes that
/// require different treatment at the graceful-degradation boundary.
///
/// - [`DeepPassError::ContractResolution`]: the bundle carries a declared
///   `logic:ReasoningContract` whose `logic:admissibleValuation` is garbled or
///   otherwise unresolvable. This is **invalid input** — the gate must HARD-FAIL
///   (no-optionality discipline). The caller emits a `Severity::Error` finding
///   and the finding code `validate.deep.contract-invalid`.
///
/// - [`DeepPassError::Unavailable`]: the semantic pass could not run for an
///   infrastructure reason (GTS read error, data parse error, reasoning engine
///   failure). The caller emits a `Severity::Note` advisory
///   (`validate.deep.unavailable`) and leaves the Tier-1 result intact (graceful
///   degradation).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
enum DeepPassError {
    /// The declared contradiction-policy contract is garbled; this is INVALID
    /// INPUT and must cause a hard-fail `Severity::Error` finding.
    ContractResolution(String),
    /// A reasoning verdict named a clash quad whose explain-skeleton derivation
    /// could not be built (the index build failed after a real verdict) or located
    /// (the witness references a quad absent from the result). This is an INTERNAL
    /// INVARIANT VIOLATION and must cause a hard-fail `Severity::Error` finding — it
    /// must NOT be downgraded to the graceful `Unavailable` advisory.
    Derivation(String),
    /// The deep pass could not run for an infrastructure / availability reason;
    /// this degrades gracefully to a `Severity::Note` advisory.
    Unavailable(String),
}

/// The blob `rep` label under which the snapshot stage folds the full SHACL shape
/// surface (`shapes-archive`). MUST match the writer in the pipeline snapshot
/// stage and the Python `bundle` reader.
const REP_SHAPES: &str = "shapes-archive";

/// Run **Tier-1** conformance of `data_bytes` (an RDF graph in `data_format`) against
/// the shapes and disciplines carried in `gts_bytes`. This is the wasm-clean core:
/// it carries no reasoner, so it compiles for `wasm32-unknown-unknown` and is the
/// sole validation surface exposed at the wasm/CLI boundary (see [`validate_json`]).
///
/// `data_format` is a media type or short format id understood by
/// [`purrdf::parse_dataset`] (`turtle`/`ttl`, `trig`, `n-triples`/`nt`,
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
/// # Errors
///
/// Returns `Err` if the bundle carries no `shapes-archive` blob, the archive is
/// malformed, the shapes fail to parse, or the data graph fails to parse.
pub fn run_tier1(
    data_bytes: &[u8],
    data_format: &str,
    gts_bytes: &[u8],
    namespace: &str,
    origin: &str,
) -> gmeow_errors::Result<Report> {
    Tier1Shapes::from_gts(gts_bytes)?.validate(data_bytes, data_format, namespace, origin)
}

/// The parsed data-graph SHACL shape union a Tier-1 run validates against,
/// decoded ONCE from a bundle's `shapes-archive` blob.
///
/// Decoding the multi-megabyte bundle and parsing the shape union dominates a
/// Tier-1 run (the per-graph validation itself takes milliseconds), so a
/// resident consumer — the MCP `validate_local` tool, a loop validating many
/// fixtures — builds this once per bundle and validates every payload against
/// it via [`Tier1Shapes::validate`]. [`run_tier1`] is the one-shot composition
/// over raw bundle bytes. Wasm-clean, like the [`run_tier1`] core it carries.
pub struct Tier1Shapes {
    shapes: purrdf::shapes::shapes::Shapes,
    /// The same data-graph shape union parsed as an [`RdfDataset`], read for each
    /// advisory shape's `logic:formalizes` provenance term during the advisory
    /// split ([`crate::advisory::split_advisory_results`]). Native-only: the
    /// advisory bridge is a native module, so the wasm Tier-1 surface never carries
    /// (or applies) the split.
    #[cfg(not(target_arch = "wasm32"))]
    shapes_dataset: Arc<RdfDataset>,
    /// The bundle's imported RDF (the ontology): the source of the formalized terms'
    /// `gmeow:howToUse` / `gmeow:useWhen` prose the native-only advisory split reads, AND
    /// (cross-platform) the class-hierarchy authority [`inject_subclass_shortcuts`] walks
    /// via [`gufo::proper_ancestors`].
    ///
    /// Tier-1 validates an external data graph IN ISOLATION (see [`Tier1Shapes::validate`]):
    /// the user's file need not restate the bundle's TBox, so it typically carries no
    /// `rdfs:subClassOf` triples at all. purrdf's SHACL engine resolves `sh:targetClass`
    /// (and value-node `sh:class`) ONLY over `rdfs:subClassOf` edges present in the graph it
    /// is validating — it never reaches into the bundle for them — so a shape targeting a
    /// superclass (e.g. `sh:targetClass math:MathematicalExpression`) silently selects NO
    /// focus node when every real instance is typed with a subclass
    /// (`math:ApplicationExpression`, `math:BindingExpression`, …). `validate` reads this
    /// field to synthesize the missing shortcut edges for exactly the classes the data graph
    /// actually uses, so the bundle's class hierarchy governs focus selection without the
    /// user needing to restate it.
    ///
    /// Unlike `shapes_dataset` above (native-only: it exists solely for the advisory-split
    /// feature), this field is NOT gated to native — every Tier-1 consumer, wasm included,
    /// needs the bundle's class hierarchy to select `sh:targetClass` focus nodes correctly
    /// over subclass-typed data.
    ontology: Arc<RdfDataset>,
}

impl Tier1Shapes {
    /// Extract and parse the data-graph shape union from raw `gmeow.gts` bytes.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the bundle carries no `shapes-archive` blob, the
    /// archive is malformed, or the shapes fail to parse.
    pub fn from_gts(gts_bytes: &[u8]) -> gmeow_errors::Result<Self> {
        let shapes_ttl = data_graph_shapes_from_gts(gts_bytes)?;
        let shapes = purrdf::shapes::engine::parse_shapes(&shapes_ttl).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("bundled SHACL shapes failed to parse: {e}"),
            })
        })?;
        // Native-only advisory-split inputs: the shape union as an RdfDataset (source
        // of each advisory shape's `logic:formalizes` provenance) and the bundle's RDF
        // (source of the formalized terms' howToUse/useWhen prose). Parsed here once
        // per bundle so a resident consumer never re-parses per payload.
        #[cfg(not(target_arch = "wasm32"))]
        let shapes_dataset = purrdf::parse_dataset(shapes_ttl.as_bytes(), "text/turtle", None)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    detail: format!("bundled SHACL shapes failed to parse as a dataset: {e}"),
                })
            })?;
        // Cross-platform (native AND wasm — see the `ontology` field doc): the bundle's
        // ontology, the class-hierarchy authority the subclass-shortcut injection reads.
        let ontology = crate::store::dataset_from_gts(gts_bytes)?;
        Ok(Self {
            shapes,
            #[cfg(not(target_arch = "wasm32"))]
            shapes_dataset,
            ontology,
        })
    }

    /// Run Tier-1 conformance of `data_bytes` (an RDF graph in `data_format`)
    /// against these shapes plus the six gUFO/OntoUML disciplines — the
    /// [`run_tier1`] core with the bundle decode hoisted out.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the data graph fails to parse.
    pub fn validate(
        &self,
        data_bytes: &[u8],
        data_format: &str,
        namespace: &str,
        origin: &str,
    ) -> gmeow_errors::Result<Report> {
        let dataset = data_dataset_flat(data_bytes, data_format)?;
        // Inject the bundle's class-hierarchy shortcuts for exactly the classes this data
        // graph uses, so `sh:targetClass` (and any SPARQL-embedded `a/<rdfs:subClassOf>*`
        // path) selects a subclass-typed focus node without the user needing to restate the
        // bundle's TBox. See the `ontology` field doc for why this is necessary.
        let dataset = inject_subclass_shortcuts(dataset, &self.ontology)?;

        let shacl_report = store::shacl_validate_dataset(&dataset, &self.shapes);

        // Split the advisory tier out of the raw SHACL results BEFORE building the flat
        // findings: an Info-severity result whose source shape carries a
        // `logic:formalizes` comes from a `logic:severity "Info"` advisory constraint
        // whose data-matching guard matched an individual. Its raw `shacl.*` finding is
        // SUPPRESSED and re-projected below as a Note + deonticRecommendation advisory
        // (the exact split the pipeline `ValidateStage` and the dev `validate_all` gate
        // apply, so the consumer `gmeow validate <file>` / MCP `validate_local` output
        // carries the same advice, not a raw `shacl.* Info` finding). Native-only: the
        // advisory bridge is a native module; the wasm surface keeps the raw report.
        #[cfg(not(target_arch = "wasm32"))]
        let (shacl_report, advisories) = crate::advisory::split_advisory_results(
            shacl_report,
            &self.shapes_dataset,
            &self.ontology,
        );

        let shacl_findings = shacl_findings_from_report(&shacl_report, Some(origin));

        let cfg = GufoConfig {
            namespace: namespace.to_owned(),
        };
        let discipline_findings = gufo::reasoning_findings(&dataset, &cfg);

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

        // Project each split advisory into a Note finding through a `DiagLedger` and
        // register its soft `Rule` (help URI) — the same dual projection the pipeline
        // `ValidateStage` and `validate_all` perform, so all three validate surfaces emit
        // identical advice from a data match. `findings("validate")` reads the whole
        // batch, so the ledger is fully attached before the flat findings are drained.
        #[cfg(not(target_arch = "wasm32"))]
        {
            use gmeow_errors::{DiagLedger, StageId};
            let mut advisory_ledger = DiagLedger::new();
            for advisory in &advisories {
                let projection = advisory.project();
                advisory_ledger.attach(projection.diag, StageId::new("validate.advisory"));
                report.add_rule(advisory.rule());
            }

            // D5 abductive tier (consumer-path twin of the pipeline / `validate_all` wiring):
            // the constructive "what to ADD" wing. The producer is ENGINE-FREE (the relatum
            // path warrants by construction, the sortal path by a sound class-disjointness
            // lookup) and only READS the graph, so it never mutates the base graph nor gates
            // the pass — every suggestion is a `Severity::Note` advisory.
            //
            // ASSERTED-VS-REASONED CONTRACT (validate_all.rs:869): a raw `gmeow validate <rdf>`
            // run is honestly ASSERTED-ONLY for the user's individuals — no reasoner is run over
            // the user graph. The producer still needs its authored `logic:AbductiveSchema`
            // vocabulary and the TBox disjointness/subclass/howToUse axioms, which live in the
            // bundle, so the abductive input is the user's parsed A-Box UNIONED with the bundle
            // ontology (`self.ontology`, the bundle's already-folded reason-stage closure). This
            // supplies the vocabulary WITHOUT fabricating any entailment over the user's data.
            let abductive_input = union_for_abductive(&self.ontology, &dataset)?;
            for suggestion in crate::abductive::abductive_advisories(&abductive_input) {
                // Attach the warrant Diag first, capturing its DiagRef, then attach the advisory
                // Diag carrying a genuine finding→finding antecedent to that warrant — the same
                // dual projection `validate_all` performs, so the abductive findings carry real
                // ledger identity (finding_iri/anchor + the findingAntecedent warrant edge) and
                // the warrant join resolves non-DARK.
                let warrant_ref =
                    advisory_ledger.attach(suggestion.warrant, StageId::new("validate.advisory"));
                let projection = suggestion.advisory.project();
                advisory_ledger.attach(
                    projection.diag.with_antecedents([warrant_ref]),
                    StageId::new("validate.advisory"),
                );
                report.add_rule(suggestion.advisory.rule());
            }

            for mut note in advisory_ledger.findings("validate") {
                // The advisory dual-projection only ever carries the focus node's
                // LOGICAL anchor (`build_advisory` sets `logical`, never `path` — it has
                // no `origin` to hand it), so patch in the physical artifact path the
                // same way the gUFO discipline findings above do. Without this, an
                // advisory Note is the one finding on this surface with no SARIF
                // `artifactLocation.uri`, which only became observable once the bundle
                // class hierarchy let a `sh:targetClass`-targeted advisory shape match a
                // subclass-typed individual instead of silently never firing.
                if let Some(loc) = note.locations.first_mut() {
                    loc.path = Some(origin.to_owned());
                } else {
                    note.add_location(Location {
                        path: Some(origin.to_owned()),
                        ..Location::default()
                    });
                }
                report.add_finding(note);
            }
        }

        Ok(report)
    }
}

/// Validate `data_bytes` (an RDF graph in `data_format`) against the bundle's
/// data-graph SHACL shapes, routing every [`ValidationResult`](purrdf::shapes::report::ValidationResult)
/// THROUGH a [`DiagLedger`](gmeow_errors::DiagLedger) so the projected [`Report`]'s
/// findings carry `related_labels` — the SHACL result-path / offending-value secondary
/// spans a multi-label consumer (the LSP's `DiagnosticRelatedInformation`) renders.
///
/// This is the SHACL-only twin of [`run_tier1`]: [`run_tier1`] hand-builds each
/// finding through [`finding_from_shacl`](crate::findings::finding_from_shacl) (which
/// carries the secondary spans only as bare `related_locations`, with no label text),
/// whereas this routes each result through [`diag_from_shacl`](crate::findings::diag_from_shacl)
/// and the ledger, so `to_finding` populates the text-bearing `related_labels` twin.
/// It runs no gUFO disciplines — the secondary-label surface is a SHACL property, and
/// the disciplines carry no result-path/value spans.
///
/// The shapes are the SAME bundle-carried data-graph shape union [`run_tier1`] uses
/// (`shapes-archive` minus the DSL/manifest lint shapes), the data is validated in
/// isolation (no ontology merge — every data-graph shape is self-contained), and named
/// graphs are flattened to the default graph. The projected report's tool is `tool`.
///
/// # Errors
///
/// Returns `Err` for the same reasons as [`run_tier1`]: the bundle carries no
/// `shapes-archive` blob, the archive is malformed, the shapes fail to parse, or the
/// data graph fails to parse.
pub fn shacl_report_via_ledger(
    data_bytes: &[u8],
    data_format: &str,
    gts_bytes: &[u8],
    tool: &str,
) -> gmeow_errors::Result<Report> {
    use gmeow_errors::{DiagLedger, StageId};

    use crate::findings::diag_from_shacl;

    let tier1 = Tier1Shapes::from_gts(gts_bytes)?;
    let dataset = data_dataset_flat(data_bytes, data_format)?;
    let shacl_report = store::shacl_validate_dataset(&dataset, &tier1.shapes);

    // The single carrier: every SHACL result interns onto ONE hash-consed ledger via
    // the ledger-native `diag_from_shacl` (which carries the result-path / offending
    // value as text-bearing `Label`s), and the projected report is its projection —
    // so each finding gains the `related_labels` the bare `finding_from_shacl` lacks.
    let mut ledger = DiagLedger::new();
    for result in &shacl_report.results {
        ledger.attach(diag_from_shacl(result), StageId::new("validate.data.shacl"));
    }
    Ok(ledger.project_report(tool))
}

/// Run Tier-1 conformance and return the [`Report`] as a JSON string — the
/// deep-less, Python-free entry for the wasm/CLI boundary.
///
/// This is the sole validation surface exposed to wasm: it wraps [`run_tier1`]
/// (never the native `--deep` path) and serializes the canonical
/// `gmeow_errors::Report` with serde_json, so a browser / editor / LLM client
/// receives structured findings without any PyO3 or filesystem coupling. Native
/// callers that want a JSON result share this same entry.
///
/// # Errors
///
/// Returns `Err` for the same Tier-1 reasons as [`run_tier1`] (missing/malformed
/// `shapes-archive`, unparsable shapes, unparsable data graph), or if the report
/// fails to serialize to JSON.
pub fn validate_json(
    data_bytes: &[u8],
    data_format: &str,
    gts_bytes: &[u8],
    namespace: &str,
    origin: &str,
) -> gmeow_errors::Result<String> {
    let report = run_tier1(data_bytes, data_format, gts_bytes, namespace, origin)?;
    serde_json::to_string(&report).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Serialize {
            detail: format!("report JSON serialization failed: {e}"),
        })
    })
}

/// Run Tier-1 conformance and, when `deep` is set, the opt-in native **Tier-2**
/// semantic pass — the consumer `gmeow validate [--deep] <data>` entry.
///
/// Tier-2 has no wasm form (it reasons via the native DL engine), so `deep` lives
/// only on this native-only wrapper; the wasm boundary reaches validation solely
/// through the deep-less [`run_tier1`] core. When `deep` is set, the semantic pass
/// reasons over the user's data MERGED with the bundle's axioms and folds the shared
/// `logic:ReasoningResult` verdict into the same report. Tier-2 degrades gracefully:
/// an infrastructure failure becomes a single `validate.deep.unavailable` advisory
/// note, leaving the complete Tier-1 result and its exit code intact.
///
/// # Errors
///
/// Returns `Err` for the same Tier-1 reasons as [`run_tier1`]. A Tier-2 (`deep`)
/// failure is NOT an error — it is folded as an advisory note.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(
    data_bytes: &[u8],
    data_format: &str,
    gts_bytes: &[u8],
    namespace: &str,
    origin: &str,
    deep: bool,
) -> gmeow_errors::Result<Report> {
    let tier1 = Tier1Shapes::from_gts(gts_bytes)?;
    let imported = purrdf::import_gts_events(gts_bytes)?;
    run_with(
        BundleParts {
            gts_bytes,
            shapes: &tier1,
            dataset: imported.dataset.as_ref(),
        },
        data_bytes,
        data_format,
        namespace,
        origin,
        deep,
    )
}

/// Borrowed views of ONE decoded `gmeow.gts` bundle — the raw bytes, the parsed
/// Tier-1 shape union, and the imported carrier dataset. All three MUST come
/// from the same bundle: the parity contract (`validate_local` ≡ `gmeow
/// validate`) holds only when the shapes, the enrichment join, and the Tier-2
/// deep pass all read the same ontology.
///
/// A resident consumer (the MCP server) decodes these once per bundle and calls
/// [`run_with`] per payload; the one-shot [`run`] decodes them per invocation.
#[cfg(not(target_arch = "wasm32"))]
pub struct BundleParts<'a> {
    /// The raw `gmeow.gts` bytes (the Tier-2 deep pass reads the bundle's blobs
    /// and axioms directly from them).
    pub gts_bytes: &'a [u8],
    /// The parsed data-graph shape union extracted from `gts_bytes`
    /// ([`Tier1Shapes::from_gts`]).
    pub shapes: &'a Tier1Shapes,
    /// The carrier dataset imported from `gts_bytes`
    /// (`purrdf::import_gts_events`), the enrichment join's bundle side.
    pub dataset: &'a RdfDataset,
}

/// The [`run`] composition with the bundle-derived artifacts supplied by the
/// caller, so a resident consumer that already holds them (the MCP
/// `validate_local` tool imports the bundle once at startup) never re-decodes
/// the whole bundle per payload. Semantics are exactly [`run`]'s: Tier-1
/// shapes and disciplines, the opt-in Tier-2 deep pass, then the
/// proof-carrying enrichment pass.
///
/// # Errors
///
/// Returns `Err` if the data graph fails to parse. A Tier-2 (`deep`) failure is
/// NOT an error — it is folded as an advisory note (see [`run`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn run_with(
    bundle: BundleParts<'_>,
    data_bytes: &[u8],
    data_format: &str,
    namespace: &str,
    origin: &str,
    deep: bool,
) -> gmeow_errors::Result<Report> {
    let mut report = bundle
        .shapes
        .validate(data_bytes, data_format, namespace, origin)?;

    // Tier-2 (`--deep`): opt-in native semantic pass over user data + bundle axioms.
    if deep {
        run_deep_pass(
            bundle.gts_bytes,
            data_bytes,
            data_format,
            origin,
            &mut report,
        );
    }

    // The single proof-carrying enrichment pass: rule identity (catalog help URIs)
    // + registry-authored remediation + per-term usage guidance on every finding, so the
    // CLI consumer report carries the same enrichment as the pipeline validate
    // stage. The bundle carries the constraint-catalog `gmeow:ValidationRule`
    // nodes (the rule-governing-term key); the user's own data graph is the
    // `documented_terms` subject. A genuinely-corrupt data graph at this point is
    // a hard input error (Tier-1 already parsed it once above), so it propagates
    // via `?` rather than being swallowed.
    let subject = data_dataset(data_bytes, data_format)?;
    crate::enrich::enrich_findings(&mut report, bundle.dataset, subject.as_ref());

    Ok(report)
}

/// Run the opt-in Tier-2 deep pass, folding either its verdict findings or — on
/// failure — an appropriate diagnostic into `report`.
///
/// This is the graceful-degradation boundary for infrastructure failures, but NOT
/// for invalid input:
///
/// - [`DeepPassError::Unavailable`]: the semantic pass could not run (GTS read
///   error, data parse error, reasoning engine failure). Folded as a single
///   `validate.deep.unavailable` `Severity::Note` advisory; the complete Tier-1
///   result and its exit code are preserved.
///
/// - [`DeepPassError::ContractResolution`]: the bundle's declared
///   `logic:ReasoningContract` carries a garbled `logic:admissibleValuation`.
///   This is INVALID INPUT (no-optionality discipline): folded as a
///   `validate.deep.contract-invalid` `Severity::Error` finding that FAILS the
///   gate. It must NOT be downgraded to an advisory note.
///
/// - [`DeepPassError::Derivation`]: a reasoning verdict referenced a clash quad
///   whose explain-skeleton derivation could not be built or located. This is an
///   INTERNAL INVARIANT VIOLATION (no-optionality discipline): folded as a
///   `validate.deep.derivation-unresolved` `Severity::Error` finding that FAILS the
///   gate. It must NOT be downgraded to an advisory note.
#[cfg(not(target_arch = "wasm32"))]
fn run_deep_pass(
    gts_bytes: &[u8],
    data_bytes: &[u8],
    data_format: &str,
    origin: &str,
    report: &mut Report,
) {
    let start = report.findings.len();
    match deep_consistency_findings(gts_bytes, data_bytes, data_format, report) {
        Ok(()) => {
            for finding in &mut report.findings[start..] {
                if finding.locations.is_empty() {
                    finding.add_location(Location {
                        path: Some(origin.to_owned()),
                        ..Location::default()
                    });
                }
            }
        }
        Err(DeepPassError::ContractResolution(msg)) => {
            // HARD FAIL: a garbled declared contract policy is invalid input;
            // it must NOT be silently downgraded to an advisory note.
            let mut finding = Finding::new(
                Severity::Error,
                crate::codes::VALIDATE_DEEP_CONTRACT_INVALID,
                format!(
                    "deep semantic pass: bundle carries a garbled \
                     logic:admissibleValuation that cannot be resolved as a \
                     contradiction policy — the gate is hard-failed: {msg}"
                ),
            )
            .with_tool("validate");
            finding.add_location(Location {
                path: Some(origin.to_owned()),
                ..Location::default()
            });
            report.add_finding(finding);
        }
        Err(DeepPassError::Derivation(msg)) => {
            // HARD FAIL: a reasoning verdict referenced a clash quad whose
            // explain-skeleton derivation could not be built or located — an
            // internal invariant violation. It must surface as a Severity::Error
            // finding, NEVER be downgraded to the graceful Unavailable note.
            let mut finding = Finding::new(
                Severity::Error,
                crate::codes::VALIDATE_DEEP_DERIVATION_UNRESOLVED,
                format!(
                    "deep semantic pass: a reasoning verdict could not be joined to its \
                     explain-skeleton derivation — the gate is hard-failed: {msg}"
                ),
            )
            .with_tool("validate");
            finding.add_location(Location {
                path: Some(origin.to_owned()),
                ..Location::default()
            });
            report.add_finding(finding);
        }
        Err(DeepPassError::Unavailable(msg)) => {
            // Graceful degradation: infrastructure/availability failure; preserve
            // the complete Tier-1 result and fold one advisory note.
            let mut finding = Finding::new(
                Severity::Note,
                crate::codes::VALIDATE_DEEP_UNAVAILABLE,
                format!("deep semantic pass skipped: {msg}"),
            )
            .with_tool("validate");
            finding.add_location(Location {
                path: Some(origin.to_owned()),
                ..Location::default()
            });
            report.add_finding(finding);
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
/// Returns [`DeepPassError::Unavailable`] if the bundle cannot be read, the user
/// data cannot be parsed into a reasoning dataset, or the native reasoning run
/// fails — all infrastructure failures that degrade gracefully.
///
/// Returns [`DeepPassError::ContractResolution`] if the bundle's declared
/// `logic:ReasoningContract` carries a garbled `logic:admissibleValuation` that
/// cannot be resolved to a [`ContradictionPolicy`]. This is INVALID INPUT and
/// must HARD-FAIL the gate; the caller emits a `Severity::Error` finding.
#[cfg(not(target_arch = "wasm32"))]
fn deep_consistency_findings(
    gts_bytes: &[u8],
    data_bytes: &[u8],
    data_format: &str,
    report: &mut Report,
) -> Result<(), DeepPassError> {
    let bundle = purrdf::import_gts_events(gts_bytes)
        .map_err(|e| DeepPassError::Unavailable(format!("GTS read error: {e}")))?;
    let user = data_dataset(data_bytes, data_format)
        .map_err(|d| DeepPassError::Unavailable(d.message().to_string()))?;
    // Narrow the bundle side to the object-level reasoning EDB — the SAME
    // boundary `crates/pipeline`'s `assemble_object_level_edb` / `stage-reason` use at
    // build time (shared via `gmeow_logic::reasoning_graphs::project_object_level_edb`)
    // — BEFORE merging in the caller's own data, so `gmeow validate <data> --deep`
    // reasons the consumer's data against byte-identical bundle worlds to the
    // pipeline's own `make reason-verify` gate rather than also reasoning over
    // meta/report graphs (documentation, diagnostics, correspondence, …) that assert
    // no object-level axioms.
    let bundle_edb = gmeow_logic::reasoning_graphs::project_object_level_edb(
        bundle.dataset.as_ref(),
    )
    .map_err(|e| DeepPassError::Unavailable(format!("object-level EDB projection failed: {e}")))?;
    let edb = {
        let mut builder = purrdf::RdfDatasetBuilder::new();
        builder.push_dataset(bundle_edb.as_ref());
        builder.push_dataset(user.as_ref());
        builder
            .freeze()
            .map_err(|e| DeepPassError::Unavailable(format!("freeze merged EDB: {e}")))?
    };
    let result = gmeow_logic::reason::reason_all(edb.as_ref())
        .map_err(|e| DeepPassError::Unavailable(format!("native reasoning failed: {e}")))?;
    // Build the faithful cited-quad-reifier derivation skeletons for the SAME result.
    // A build failure AFTER the reasoner produced a real verdict is an internal
    // invariant violation (a cycle or unresolved antecedent in the proof trace), NOT
    // an infrastructure availability failure: it maps to the hard-fail `Derivation`
    // variant, never the graceful `Unavailable` note.
    let explanations = gmeow_logic::explain::explanations_for_result(&result).map_err(|e| {
        DeepPassError::Derivation(format!(
            "explanation-skeleton build failed after a real verdict (internal invariant): {e}"
        ))
    })?;
    // The governing contradiction policy is READ from the bundle's declared
    // logic:ReasoningContract (logic:admissibleValuation), not pinned: no contract /
    // no valuation ⇒ conservative classical DEFAULT (a glut IS owl:Nothing); multiple
    // conflicting valuations ⇒ the MOST CONSERVATIVE governs; a garbled valuation
    // HARD-FAILS rather than silently relaxing the gate. The policy is read off the
    // bundle (the authority for the contract), not the user-supplied data graph.
    //
    // NOTE: this is the ONLY error that maps to ContractResolution (not Unavailable)
    // — a garbled contract is invalid INPUT, not an infrastructure failure, and must
    // produce a Severity::Error finding rather than being silently downgraded.
    let policy = gmeow_logic::certificate::ContradictionPolicy::resolve_from_dataset(
        bundle.dataset.as_ref(),
    )
    .map_err(|e| DeepPassError::ContractResolution(format!("contract resolution failed: {e}")))?;
    crate::validate_all::fold_reasoning_result(&result, policy, &explanations, report)
        .map_err(|e| DeepPassError::Derivation(e.message))?;

    // The math: dimensional-homogeneity + math: expression-identity reasoned gates
    // the SAME two checks `stage-verify` / `gmeow-dev reason-verify` run at
    // build time over the pipeline's own `assemble_object_level_edb`, now reachable
    // from `gmeow validate --deep` over the CALLER'S OWN data merged with the bundle
    // — a consumer with their own math AST graph gets `math:StructuralKeyDrift` /
    // `math:FalseStructuralNormalizationClaim` findings directly from the `gmeow`
    // CLI, not only from the MCP `verify_graph` tool. Deliberately narrower than the
    // FULL `gmeow_logic::verify::verify_with_reasoning_result` battery: that also
    // runs the embedded `queries/verify/*.rq` bad-example queries, several of which
    // check for FIXED gmeow vocabulary (e.g. `axis-not-disjoint`'s seven
    // identity-axis classes) that only the real production bundle carries —
    // misfiring on a caller's own PARTIAL data graph unioned with a non-production
    // bundle. The math: gates carry no such fixed-vocabulary assumption. Runs over
    // the SAME `edb` + `result` the consistency fold above just used.
    match gmeow_logic::verify::materialize_reasoned_graph(edb.as_ref(), &result).map_err(|e| {
        DeepPassError::Unavailable(format!("reasoned-graph materialization failed: {e}"))
    })? {
        gmeow_logic::verify::ReasonedGraphOutcome::Ready(reasoned) => {
            for finding in gmeow_logic::math_dimension::check_math_dimension_findings(
                reasoned.dataset.as_ref(),
            ) {
                report.add_finding(finding);
            }
            for finding in gmeow_logic::math_expression::check_math_expression_findings(
                edb.as_ref(),
                reasoned.dataset.as_ref(),
            ) {
                report.add_finding(finding);
            }
        }
        gmeow_logic::verify::ReasonedGraphOutcome::IncompleteClosure(findings) => {
            for finding in findings {
                report.add_finding(finding);
            }
        }
    }
    Ok(())
}

/// Parse external RDF data bytes into a graph-preserving [`RdfDataset`] for the
/// Tier-2 reasoner (the world structure must survive, so this does NOT flatten the
/// way [`data_store`] does for SHACL). Handles every supported format, routing
/// JSON-LD through the gmeow-gts codec exactly as [`data_store`] does.
#[cfg(not(target_arch = "wasm32"))]
fn data_dataset(data_bytes: &[u8], data_format: &str) -> gmeow_errors::Result<Arc<RdfDataset>> {
    if is_json_ld(data_format) {
        // JSON-LD has no native-codec media type; route it through the FIRST-PARTY
        // native JSON-LD-star codec, which folds the RDF 1.2 statement layer and
        // PRESERVES named graphs — the graph-preserving shape this Tier-2 path needs
        // (no longer the external gmeow-gts JSON-LD codec).
        return purrdf::native_codecs::jsonld::parse_jsonld(data_bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("JSON-LD parse error: {e}"),
            })
        });
    }
    purrdf::parse_dataset(data_bytes, data_format, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: located_parse_error("data graph parse error", &e),
        })
    })
}

/// Render a parse [`purrdf::RdfDiagnostic`] as a hard-fail message that surfaces the
/// source location (line/column) purrdf records on the diagnostic but its `Display`
/// omits — so a malformed data graph reports *where* it broke, not just *that* it did.
fn located_parse_error(context: &str, diagnostic: &purrdf::RdfDiagnostic) -> String {
    let at = diagnostic
        .location
        .as_ref()
        .and_then(|loc| match (loc.line, loc.column) {
            (Some(line), Some(column)) => Some(format!(" at line {line}, column {column}")),
            (Some(line), None) => Some(format!(" at line {line}")),
            _ => None,
        })
        .unwrap_or_default();
    format!("{context}{at}: {diagnostic}")
}

/// Build a frozen native [`RdfDataset`] from external RDF data bytes, flattening any
/// named graphs into the default graph so the shapes and discipline checks see the
/// whole graph. (Tier-1 SHACL; the Tier-2 reasoner uses the graph-preserving
/// [`data_dataset`] above.)
fn data_dataset_flat(
    data_bytes: &[u8],
    data_format: &str,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    if is_json_ld(data_format) {
        // JSON-LD has no native-codec media type; route it through the FIRST-PARTY
        // native JSON-LD-star codec, then re-home every named graph to the default graph
        // (the Tier-1 SHACL path needs the whole graph flat). This matches the prior
        // gmeow-gts → `dataset_from_gts` flattening behavior.
        let dataset = purrdf::native_codecs::jsonld::parse_jsonld(data_bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("JSON-LD parse error: {e}"),
            })
        })?;
        return flatten_to_default_graph(&dataset);
    }

    // Parse to the native IR, then re-home every named graph to the default graph so
    // the flattened graph matches the old `FlattenToDefaultGraph` store.
    let dataset = purrdf::parse_dataset(data_bytes, data_format, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: located_parse_error("data graph parse error", &e),
        })
    })?;
    flatten_to_default_graph(&dataset)
}

/// Re-home every quad of `dataset` to the default graph (the native twin of
/// `GraphPolicy::FlattenToDefaultGraph`), returning a fresh frozen dataset.
fn flatten_to_default_graph(dataset: &RdfDataset) -> gmeow_errors::Result<Arc<RdfDataset>> {
    use purrdf::RdfDatasetBuilder;
    let mut builder = RdfDatasetBuilder::new();
    for mut quad in dataset.owned_quads() {
        quad.graph_name = None;
        builder.push_owned_quad(&quad);
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Dataset {
            detail: format!("flatten data graph to default graph: {e}"),
        })
    })
}

/// Synthesize and merge the bundle's class-hierarchy shortcut edges for exactly the
/// distinct `rdf:type` classes `dataset` uses, so `sh:targetClass` (SHACL's own
/// `rdfs:subClassOf` closure) selects a subclass-typed focus node without the data graph
/// needing to restate the bundle's TBox.
///
/// For each distinct type IRI `C` the flattened data graph asserts via `rdf:type`, this
/// walks `C`'s full transitive superclass set in `ontology` (via [`gufo::proper_ancestors`],
/// which follows BOTH `rdfs:subClassOf` and `logic:subClassOf`) and adds one direct
/// `C rdfs:subClassOf A` SHORTCUT edge per ancestor `A` — collapsing any multi-hop bundle
/// chain to a single hop, so the engine's own `sh:targetClass` resolution selects
/// `C`-typed focus nodes for every shape targeting any ancestor of `C`, exactly as the
/// merged-dataset dev-authoring gate (`validate_all`) already does.
///
/// Cost is proportional to the number of DISTINCT types the data graph actually uses (not
/// to the bundle's whole class hierarchy, and never per focus node): `ontology` is already
/// decoded once per bundle load, so this pays one ancestor walk per distinct type, not one
/// per instance.
///
/// # Errors
///
/// Returns `Err` if the synthesized shortcut Turtle fails to parse (an internal invariant
/// violation — every synthesized IRI is a term the ontology dataset already accepted) or
/// the merge fails to freeze.
fn inject_subclass_shortcuts(
    dataset: Arc<RdfDataset>,
    ontology: &RdfDataset,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let Some(type_id) = dataset.term_id_by_value(&TermValue::iri(crate::model::rdf::TYPE)) else {
        return Ok(dataset);
    };
    let mut used_types: BTreeSet<String> = BTreeSet::new();
    for quad in dataset.quads_for_pattern(None, Some(type_id), None, GraphMatch::Any) {
        if let TermRef::Iri(class_iri) = dataset.resolve(quad.o) {
            used_types.insert(class_iri.to_owned());
        }
    }
    if used_types.is_empty() {
        return Ok(dataset);
    }

    let mut shortcuts = String::new();
    for class_iri in &used_types {
        let mut ancestors: Vec<String> = gufo::proper_ancestors(ontology, class_iri)
            .into_iter()
            .collect();
        ancestors.sort();
        for ancestor in ancestors {
            shortcuts.push('<');
            shortcuts.push_str(class_iri);
            shortcuts.push_str("> <");
            shortcuts.push_str(crate::model::rdfs::SUB_CLASS_OF);
            shortcuts.push_str("> <");
            shortcuts.push_str(&ancestor);
            shortcuts.push_str("> .\n");
        }
    }
    if shortcuts.is_empty() {
        return Ok(dataset);
    }

    let shortcut_dataset = purrdf::parse_dataset(shortcuts.as_bytes(), "text/turtle", None)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("internal subclass-shortcut synthesis failed to parse: {e}"),
            })
        })?;

    use purrdf::RdfDatasetBuilder;
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(&dataset);
    builder.push_dataset(&shortcut_dataset);
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Dataset {
            detail: format!("subclass-shortcut merge failed: {e}"),
        })
    })
}

/// Build the abductive producer's input graph: the bundle `ontology` (its authored
/// `logic:AbductiveSchema` vocabulary + the TBox disjointness/subclass/howToUse axioms,
/// carrying the folded reason-stage closure) UNIONED with the user's parsed A-Box
/// `data` graph. The union supplies the producer the vocabulary it needs to discover
/// schemas and refute sortals WITHOUT running any reasoner over the user's data — the
/// honest ASSERTED-ONLY consumer surface (validate_all.rs:869). Each side is pushed
/// under a fresh blank scope; the frozen result is only READ by the producer.
#[cfg(not(target_arch = "wasm32"))]
fn union_for_abductive(
    ontology: &RdfDataset,
    data: &RdfDataset,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    use purrdf::RdfDatasetBuilder;
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(ontology);
    builder.push_dataset(data);
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Dataset {
            detail: format!("union bundle ontology with user data for the abductive tier: {e}"),
        })
    })
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
fn data_graph_shapes_from_gts(gts_bytes: &[u8]) -> gmeow_errors::Result<String> {
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
            gmeow_errors::Diag::of_kind(crate::error::Dataset {
                detail: format!(
                    "bundle carries no `{REP_SHAPES}` blob — cannot validate repo-free"
                ),
            })
        })?;

    // Decode the blob bytes (forcing a lazy entry if the fold deferred it).
    let entry = graph
        .blobs
        .iter_mut()
        .find(|(d, _)| *d == digest)
        .map(|(_, e)| e)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Dataset {
                detail: format!("`{REP_SHAPES}` blob metadata present but bytes missing"),
            })
        })?;
    let tar = entry
        .decode()
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Dataset {
                detail: format!("`{REP_SHAPES}` blob decode error: {e}"),
            })
        })?
        .to_vec();

    let mut members = purrdf::ustar::read_archive(&tar)
        .map_err(|e| gmeow_errors::Diag::of_kind(crate::error::Dataset { detail: e }))?;
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
        let text = std::str::from_utf8(bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Dataset {
                detail: format!("shape `{name}` is not valid UTF-8: {e}"),
            })
        })?;
        ttl.push_str(text);
        ttl.push('\n');
        included += 1;
    }

    if included == 0 {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Dataset {
            detail: format!(
                "`{REP_SHAPES}` blob held no data-graph shapes — the bundle is incomplete"
            ),
        }));
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
        if let ciborium::value::Value::Text(name) = k
            && name == key
        {
            if let ciborium::value::Value::Text(text) = v {
                return Some(text.as_str());
            }
            return None;
        }
    }
    None
}

// The deep-pass tests exercise `run_deep_pass`, which is native-only; the whole
// module is gated to the native target so a wasm `--all-targets` pass stays clean.
#[cfg(all(test, not(target_arch = "wasm32")))]
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
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "validate.deep.inconsistent")
        );
    }

    /// Build canonical GTS bytes from an arbitrary Turtle string for use in
    /// deep-pass tests. Mirrors the same helper in `validate_all` tests.
    fn gts_bytes_from_turtle(ttl: &str) -> Vec<u8> {
        let dataset =
            purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse test turtle");
        purrdf::gts_write::to_gts(
            &dataset,
            &purrdf::RdfLookaside::default(),
            "gmeow-validate-data-deep-test",
        )
        .expect("encode GTS bytes")
    }

    /// Regression guard for the hard-fail discipline: a bundle whose declared
    /// `logic:ReasoningContract` carries a GARBLED `logic:admissibleValuation`
    /// (here `logic:Nonsense`, an unrecognised local name) must produce a
    /// `Severity::Error` finding with code `validate.deep.contract-invalid`, NOT
    /// a `validate.deep.unavailable` advisory Note. The gate must FAIL.
    ///
    /// This test catches the defect where `run_deep_pass` was collapsing both
    /// failure modes (invalid input and infrastructure unavailability) into a
    /// single non-failing advisory, silently passing a bundle with invalid data.
    #[test]
    fn deep_pass_garbled_contract_produces_error_not_advisory() {
        let garbled_bundle = gts_bytes_from_turtle(
            "\
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
logic:c rdf:type logic:ReasoningContract ;
    logic:admissibleValuation logic:Nonsense .
",
        );

        let mut report = Report::new("validate");
        run_deep_pass(
            &garbled_bundle,
            b"<http://example.org/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/T> .\n",
            "n-triples",
            "fixture.nt",
            &mut report,
        );

        // Must NOT fold an advisory note — that is the defect this test guards against.
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "validate.deep.unavailable"),
            "a garbled contract must NOT produce an advisory note (validate.deep.unavailable); \
             it is invalid INPUT, not an availability failure: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );

        // Must fold a hard-fail Error finding.
        let contract_error: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.code == "validate.deep.contract-invalid")
            .collect();
        assert_eq!(
            contract_error.len(),
            1,
            "exactly one validate.deep.contract-invalid error must be emitted: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
        assert_eq!(
            contract_error[0].severity,
            Severity::Error,
            "a garbled contract policy must be a hard-fail Error finding"
        );
        assert_eq!(
            contract_error[0]
                .locations
                .first()
                .and_then(|l| l.path.as_deref()),
            Some("fixture.nt"),
            "the contract-invalid finding must carry the origin path"
        );
        assert!(
            !report.ok(),
            "a garbled contract policy must fail the gate (report.ok() must be false)"
        );
    }

    #[test]
    fn report_json_round_trips() {
        // The wasm/CLI boundary (`validate_json`) serializes a Report to JSON; this
        // guards that the canonical Report model round-trips through serde_json so a
        // client can parse the findings back losslessly.
        let mut report = Report::new("validate");
        report.add_finding(
            Finding::new(Severity::Error, "tier1.fixture", "a fixture finding")
                .with_tool("validate"),
        );
        let json = serde_json::to_string(&report).expect("Report must serialize to JSON");
        let back: Report = serde_json::from_str(&json).expect("Report JSON must deserialize back");
        assert_eq!(
            report, back,
            "Report must round-trip through JSON unchanged"
        );
    }

    #[test]
    fn validate_json_surfaces_missing_shapes_as_err_string() {
        // A plain GTS bundle carries no `shapes-archive` blob, so the wasm/CLI entry
        // must return an Err STRING (not panic) that names the missing surface — the
        // no-optionality hard-fail surfaced as a boundary-friendly error.
        let bundle =
            gts_bytes_from_turtle("@prefix ex: <http://example.org/> .\nex:a ex:b ex:c .\n");
        let err = validate_json(
            b"<http://example.org/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/T> .\n",
            "n-triples",
            &bundle,
            "https://blackcatinformatics.ca/gmeow/",
            "fixture.nt",
        )
        .expect_err("a bundle without a shapes-archive must be an Err");
        assert!(err.is::<crate::error::Dataset>());
        assert!(
            err.message().contains("shapes-archive"),
            "the error must name the missing bundle surface: {}",
            err.message()
        );
    }

    /// Build a `Tier1Shapes` directly from hand-authored shape + ontology Turtle,
    /// bypassing the bundle decode — a self-contained fixture proving
    /// [`Tier1Shapes::validate`] WIRES the advisory split without needing a real
    /// `gmeow.gts`. `shapes_ttl` feeds BOTH the SHACL engine (`shapes`) and the
    /// `logic:formalizes` provenance reader (`shapes_dataset`); `ontology_ttl` carries
    /// the formalized term's howToUse/useWhen prose.
    fn tier1_from_ttl(shapes_ttl: &str, ontology_ttl: &str) -> Tier1Shapes {
        let shapes = purrdf::shapes::engine::parse_shapes(shapes_ttl).expect("shapes parse");
        let shapes_dataset =
            purrdf::parse_dataset(shapes_ttl.as_bytes(), "text/turtle", None).expect("shapes ds");
        let ontology = purrdf::parse_dataset(ontology_ttl.as_bytes(), "text/turtle", None)
            .expect("ontology ds");
        Tier1Shapes {
            shapes,
            shapes_dataset,
            ontology,
        }
    }

    /// The consumer-path wiring proof (F1): `Tier1Shapes::validate` — the shared core
    /// `gmeow validate <file>` and the MCP `validate_local` tool both reach — applies the
    /// advisory split. A bare `gmeow:Entity` individual (the anti-pattern the Info-severity
    /// advisory guard matches) must surface as a `Severity::Note`, `advice.*` finding
    /// carrying the formalized term's howToUse suggestion and a "Use when:" useWhen entry —
    /// NOT a raw `shacl.* Info` finding for that shape.
    #[test]
    fn validate_wires_the_advisory_split_for_a_bare_entity() {
        const SHAPES: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
<https://ex.test/EntityAdviceShape> a sh:NodeShape ;
    logic:formalizes gmeow:Entity ;
    sh:targetClass gmeow:Entity ;
    sh:sparql [
        a sh:SPARQLConstraint ;
        sh:severity sh:Info ;
        sh:message "prefer a more specific sortal than bare gmeow:Entity" ;
        sh:select "SELECT $this WHERE { $this a <https://blackcatinformatics.ca/gmeow/Entity> }" ;
    ] .
"#;
        const ONTOLOGY: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:Entity gmeow:howToUse \"Type each instance with its most specific sortal.\"@x-gmeow-english ;
    gmeow:useWhen \"Use for a genuinely category-neutral resource.\"@x-gmeow-english .
";
        let tier1 = tier1_from_ttl(SHAPES, ONTOLOGY);
        let data = "<https://ex.test/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                    <https://blackcatinformatics.ca/gmeow/Entity> .\n";
        let report = tier1
            .validate(
                data.as_bytes(),
                "n-triples",
                "https://blackcatinformatics.ca/gmeow/",
                "user-data.ttl",
            )
            .expect("Tier-1 validate must succeed");

        // The advisory is a Note, code in the advice.* family.
        let advice: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.code.starts_with(crate::codes::ADVICE_FAMILY))
            .collect();
        assert_eq!(
            advice.len(),
            1,
            "exactly one advice.* Note finding must be wired in by validate: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
        let advice = advice[0];
        assert_eq!(advice.severity, Severity::Note);

        // The raw shacl.* Info finding for the advisory shape must have been SUPPRESSED.
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code.starts_with(crate::codes::SHACL_FAMILY)
                    && f.severity == Severity::Info),
            "the raw shacl.* Info finding must be suppressed once split into advice: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );

        // howToUse populates the suggestions verbatim; useWhen surfaces as guidance.
        assert!(
            advice
                .suggestions
                .iter()
                .any(|s| s == "Type each instance with its most specific sortal."),
            "the advice must carry the term's gmeow:howToUse as a suggestion: {:?}",
            advice.suggestions
        );
        assert!(
            advice
                .suggestions
                .iter()
                .any(|s| s == "Use when: Use for a genuinely category-neutral resource."),
            "the advice must carry the term's gmeow:useWhen as a \"Use when:\" entry: {:?}",
            advice.suggestions
        );
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

    /// The shared subclass-hierarchy fixture for the [`inject_subclass_shortcuts`] proofs
    /// below: `ex:A ⊑ ex:B ⊑ ex:C` (a two-hop chain, so a one-hop-only fix would fail the
    /// transitivity proof), plus an unrelated `ex:D` with no subsumption edge to `ex:A`.
    const SUBCLASS_ONTOLOGY: &str = "\
@prefix ex: <https://ex.test/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:A rdfs:subClassOf ex:B .
ex:B rdfs:subClassOf ex:C .
";

    /// A `sh:targetClass` shape requiring `ex:name` on instances of `class_local`
    /// (e.g. `C`, `D`) — the pattern that is dead when every real instance is
    /// typed with a proper subclass rather than the targeted class itself.
    fn subclass_probe_shapes(class_local: &str) -> String {
        format!(
            "\
@prefix ex: <https://ex.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
<https://ex.test/{class_local}Shape> a sh:NodeShape ;
    sh:targetClass ex:{class_local} ;
    sh:property [ sh:path ex:name ; sh:minCount 1 ] .
"
        )
    }

    /// Bundle-hierarchy regression: a focus node typed ONLY as a proper subclass (`ex:x a
    /// ex:A`, never `ex:x a ex:C` directly) IS selected by a shape whose `sh:targetClass`
    /// names an ANCESTOR (`ex:C`) the isolated data graph never restates — the exact defect
    /// the shipped `gmeow validate <file>` CLI hit on `math:ArgumentSlotContiguityConstraint`
    /// (`sh:targetClass math:MathematicalExpression` never selecting an
    /// `math:ApplicationExpression`-typed root). Without [`inject_subclass_shortcuts`], this
    /// finding is silently absent because the isolated data graph carries no
    /// `rdfs:subClassOf` triple at all.
    #[test]
    fn subclass_typed_focus_node_is_selected_across_the_bundle_hierarchy() {
        let tier1 = tier1_from_ttl(&subclass_probe_shapes("C"), SUBCLASS_ONTOLOGY);
        let data = "<https://ex.test/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                    <https://ex.test/A> .\n";
        let report = tier1
            .validate(
                data.as_bytes(),
                "n-triples",
                "https://ex.test/",
                "user-data.ttl",
            )
            .expect("Tier-1 validate must succeed");

        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code.starts_with(crate::codes::SHACL_FAMILY)),
            "a node typed only as a proper subclass of the shape's sh:targetClass must still \
             be selected as a focus node (missing ex:name must be flagged): {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
    }

    /// Bundle-hierarchy regression, the negative twin: a shape targeting an UNRELATED class
    /// (`ex:D`, no subsumption edge to/from `ex:A` in [`SUBCLASS_ONTOLOGY`]) must NOT select
    /// an `ex:A`-typed focus node — the shortcut injection must not over-approximate and
    /// select every instance for every shape regardless of its real class.
    #[test]
    fn unrelated_class_shape_is_not_selected() {
        let tier1 = tier1_from_ttl(&subclass_probe_shapes("D"), SUBCLASS_ONTOLOGY);
        let data = "<https://ex.test/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                    <https://ex.test/A> .\n";
        let report = tier1
            .validate(
                data.as_bytes(),
                "n-triples",
                "https://ex.test/",
                "user-data.ttl",
            )
            .expect("Tier-1 validate must succeed");

        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code.starts_with(crate::codes::SHACL_FAMILY)),
            "a shape targeting an unrelated, unconnected class must select NO focus node: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
    }

    /// Bundle-hierarchy regression, the transitivity proof: the shape targets `ex:C`, the data
    /// is typed only `ex:A`, and [`SUBCLASS_ONTOLOGY`] connects them ONLY via the two-hop
    /// chain `ex:A ⊑ ex:B ⊑ ex:C` — `ex:A` carries no DIRECT `rdfs:subClassOf ex:C` edge, so
    /// this fails if the shortcut injection only walked one hop instead of the full
    /// transitive ancestor set ([`gufo::proper_ancestors`], the same BFS the OntoUML
    /// disciplines already trust).
    #[test]
    fn subclass_shortcut_injection_is_transitive_across_two_hops() {
        // Sanity: the fixture really is two hops, not a direct edge (guards against a
        // fixture typo silently turning this into the single-hop test above).
        let ontology = purrdf::parse_dataset(SUBCLASS_ONTOLOGY.as_bytes(), "text/turtle", None)
            .expect("ontology parses");
        assert!(
            !gufo::proper_ancestors(&ontology, "https://ex.test/A").is_empty(),
            "fixture sanity: ex:A must have at least one ancestor"
        );
        assert!(
            SUBCLASS_ONTOLOGY
                .lines()
                .filter(|l| l.contains("ex:A") && l.contains("ex:C"))
                .count()
                == 0,
            "fixture sanity: ex:A must NOT carry a direct edge to ex:C"
        );

        let tier1 = tier1_from_ttl(&subclass_probe_shapes("C"), SUBCLASS_ONTOLOGY);
        let data = "<https://ex.test/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                    <https://ex.test/A> .\n";
        let report = tier1
            .validate(
                data.as_bytes(),
                "n-triples",
                "https://ex.test/",
                "user-data.ttl",
            )
            .expect("Tier-1 validate must succeed");

        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code.starts_with(crate::codes::SHACL_FAMILY)),
            "a two-hop transitive ancestor (ex:A ⊑ ex:B ⊑ ex:C) must still be reached by the \
             shortcut injection: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
    }
}
