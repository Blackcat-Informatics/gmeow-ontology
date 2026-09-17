// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
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

/// Producer-prepared native laws used by a selected deep validation pass.
#[cfg(not(target_arch = "wasm32"))]
pub use gmeow_logic::verify::PreparedReasonedGates;

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
    /// Required producer-prepared laws are absent, corrupt, or from another source
    /// identity. A selected semantic capability must fail closed.
    NativeGates(String),
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
/// A resident consumer builds this once per bundle and validates every payload
/// against the same prepared shapes via [`Tier1Shapes::validate`], avoiding
/// repeated bundle decoding and shape parsing. [`run_tier1`] is the one-shot composition
/// over raw bundle bytes. Wasm-clean, like the [`run_tier1`] core it carries.
pub struct Tier1Shapes {
    shapes: purrdf::shapes::shapes::Shapes,
    /// The shapes graph's `shape resource → gmeow:enforcesFailureClass` index, built
    /// ONCE per bundle. Every Tier-1 finding is resolved through it so it NAMES the
    /// typed conformance failure its violated law declares, instead of shipping only
    /// the generic constraint-component code (which every gate of that shape shares).
    failure_classes: crate::findings::FailureClassIndex,
    /// The bundle's imported RDF (the ontology): the source of the formalized terms'
    /// `gmeow:howToUse` / `gmeow:useWhen` prose the advisory split reads, AND
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
    /// Every Tier-1 consumer, wasm included, needs this hierarchy to select
    /// `sh:targetClass` focus nodes correctly over subclass-typed data.
    ontology: Arc<RdfDataset>,
}

impl Tier1Shapes {
    /// Borrow the exact parsed shape set used by this resident validator.
    ///
    /// Shape identities in findings refer to this set. Introspection shares its
    /// native values and blank-node identities without parsing the archive again.
    pub fn parsed_shapes(&self) -> &purrdf::shapes::shapes::Shapes {
        &self.shapes
    }

    /// Extract and parse the data-graph shape union from raw `gmeow.gts` bytes.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the bundle carries no `shapes-archive` blob, the
    /// archive is malformed, or the shapes fail to parse.
    pub fn from_gts(gts_bytes: &[u8]) -> gmeow_errors::Result<Self> {
        let imported = import_validation_bundle(gts_bytes, false)?;
        Self::from_imported(&imported)
    }

    fn from_imported(imported: &purrdf::GtsImportWithBlobs) -> gmeow_errors::Result<Self> {
        let archive = gmeow_gts_profile::archive::required_imported_blob(imported, REP_SHAPES)?;
        let shapes_ttl = data_graph_shapes_from_archive(&archive.bytes)?;
        Self::from_shapes_and_ontology(&shapes_ttl, Arc::clone(&imported.bundle.dataset))
    }

    /// Build the resident Tier-1 view from an already-authenticated shape union and
    /// already-imported ontology dataset.
    ///
    /// This is the read-only composition used after an explicit producer has published
    /// both intermediates. It avoids decoding the same GTS container again merely to
    /// recover bytes and indexes the caller already authenticated.
    pub fn from_shapes_and_ontology(
        shapes_ttl: &str,
        ontology: Arc<RdfDataset>,
    ) -> gmeow_errors::Result<Self> {
        let shapes = purrdf::shapes::engine::parse_shapes(shapes_ttl, None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("bundled SHACL shapes failed to parse: {e}"),
            })
        })?;
        // Read the exact immutable graph retained by the SHACL parser. Its
        // blank identities and document-prefix handling govern these same shapes.
        let failure_classes =
            crate::findings::FailureClassIndex::from_shapes_dataset(shapes.dataset());
        Ok(Self {
            shapes,
            failure_classes,
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
        self.validate_dataset(dataset, namespace, origin)
    }

    /// Validate the request's already parsed flat view with the resident shapes.
    fn validate_dataset(
        &self,
        dataset: Arc<RdfDataset>,
        namespace: &str,
        origin: &str,
    ) -> gmeow_errors::Result<Report> {
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
        // carries the same advice, not a raw `shacl.* Info` finding). Applied on every
        // target: the advisory bridge is wasm-clean, so the browser-run `validate_local`
        // splits the report exactly as the native CLI does — there is no target on which
        // the raw `shacl.* Info` findings survive.
        let (shacl_report, advisories) = crate::advisory::split_advisory_results(
            shacl_report,
            self.shapes.dataset(),
            &self.ontology,
        );

        let shacl_findings =
            shacl_findings_from_report(&shacl_report, Some(origin), &self.failure_classes);

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
        ledger.attach(
            diag_from_shacl(result, &tier1.failure_classes),
            StageId::new("validate.data.shacl"),
        );
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
/// Returns `Err` when the bundle dataset, shapes or Tier-1 inputs cannot be admitted.
/// Missing or invalid deep laws produce a hard report error alongside Tier-1 findings.
/// A failed deep archive admission permits one independent Tier-1 admission solely
/// to report those findings; the original deep failure remains mandatory and cannot
/// become a successful shallow result. Valid deep inputs are imported only once.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(
    data_bytes: &[u8],
    data_format: &str,
    gts_bytes: &[u8],
    namespace: &str,
    origin: &str,
    deep: bool,
) -> gmeow_errors::Result<Report> {
    let (imported, deep_admission_error) = match import_validation_bundle(gts_bytes, deep) {
        Ok(imported) => (imported, None),
        Err(error) if deep => {
            // Error reporting only: preserve Tier-1 diagnostics while retaining the
            // original failure as a hard deep contract finding. This does not retry
            // or weaken the requested semantic operation.
            (import_validation_bundle(gts_bytes, false)?, Some(error))
        }
        Err(error) => return Err(error),
    };
    let tier1 = Tier1Shapes::from_imported(&imported)?;
    let gates = deep
        .then(|| match deep_admission_error {
            Some(error) => Err(error),
            None => crate::validate_all::prepared_imported_gates(&imported),
        })
        .transpose();
    run_with(
        BundleParts {
            native_gates: match gates {
                Ok(ref selected) => selected.as_ref().map(Ok),
                Err(error) => Some(Err(error)),
            },
            shapes: &tier1,
            dataset: imported.bundle.dataset.as_ref(),
        },
        data_bytes,
        data_format,
        namespace,
        origin,
        deep,
    )
}

/// Borrowed views of ONE decoded `gmeow.gts` bundle — prepared native laws, the
/// Tier-1 shape union, and the imported carrier dataset. All three MUST come
/// from the same bundle: the parity contract (`validate_local` ≡ `gmeow
/// validate`) holds only when the shapes, the enrichment join, and the Tier-2
/// deep pass all read the same ontology.
///
/// A resident consumer (the MCP server) decodes these once per bundle and calls
/// [`run_with`] per payload; the one-shot [`run`] decodes them per invocation.
pub struct BundleParts<'a> {
    /// Exact prepared native laws for a selected deep pass. Shallow callers leave
    /// this absent; deep callers must supply the admitted laws or their load error.
    /// No archive reader or corpus producer is reachable from this composition.
    #[cfg(not(target_arch = "wasm32"))]
    pub native_gates: Option<gmeow_errors::Result<&'a PreparedReasonedGates>>,
    /// The parsed data-graph shape union from the same admitted bundle.
    pub shapes: &'a Tier1Shapes,
    /// The native graph-preserving carrier, shared by enrichment and deep reasoning.
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
/// Returns `Err` if the data graph fails to parse. Deep execution failures retain
/// the classification described by [`run`], including hard findings for missing laws.
///
/// # The `deep` leg on a target with no reasoner
///
/// The Tier-2 semantic pass IS the native DL engine (`gmeow-logic`), which the Tier-1
/// wasm validator image must never carry. On `wasm32` a `deep: true` request is
/// therefore a NAMED HARD ERROR rather than a silently-shallow pass: a caller that
/// asked for the semantic tier and got only Tier-1 back, with no way to tell, would be
/// exactly the silent capability degradation the report contract forbids. `deep: false`
/// — the whole Tier-1 + advisory + abductive + enrichment composition — runs
/// identically on both targets.
pub fn run_with(
    bundle: BundleParts<'_>,
    data_bytes: &[u8],
    data_format: &str,
    namespace: &str,
    origin: &str,
    deep: bool,
) -> gmeow_errors::Result<Report> {
    let subject = data_dataset(data_bytes, data_format)?;
    let flat = flatten_to_default_graph(&subject)?;
    let mut report = bundle.shapes.validate_dataset(flat, namespace, origin)?;

    // Tier-2 (`--deep`): opt-in native semantic pass over user data + bundle axioms.
    if deep {
        #[cfg(not(target_arch = "wasm32"))]
        run_deep_pass(
            bundle.native_gates,
            bundle.dataset,
            &subject,
            origin,
            &mut report,
        );
        #[cfg(target_arch = "wasm32")]
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: "the Tier-2 semantic pass (`deep`) requires the native DL reasoning \
                     engine, which this target does not carry; re-run with `deep` unset for \
                     the full Tier-1 + advisory + abductive + enrichment pass"
                .to_owned(),
        }));
    }

    // The single proof-carrying enrichment pass: rule identity (catalog help URIs)
    // + registry-authored remediation + per-term usage guidance on every finding, so the
    // CLI consumer report carries the same enrichment as the pipeline validate
    // stage. The bundle carries the constraint-catalog `gmeow:ValidationRule`
    // nodes (the rule-governing-term key); the user's own data graph is the
    // `documented_terms` subject. The same graph-preserving user dataset fed
    // Tier 1's flat projection and, when selected, native reasoning.
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
///
/// - [`DeepPassError::NativeGates`]: required bundle-carried native verification
///   laws are missing, corrupt, or bound to a different source identity. Folded as
///   a `validate.deep.contract-invalid` `Severity::Error`; these mandatory laws
///   cannot be silently omitted from a selected deep pass.
#[cfg(not(target_arch = "wasm32"))]
fn run_deep_pass(
    native_gates: Option<gmeow_errors::Result<&PreparedReasonedGates>>,
    bundle: &RdfDataset,
    user: &RdfDataset,
    origin: &str,
    report: &mut Report,
) {
    let start = report.findings.len();
    let outcome = deep_consistency_findings(native_gates, bundle, user, report);
    fold_deep_outcome(outcome, start, origin, report);
}

/// Apply the selected deep pass's diagnostic classification without changing
/// findings already emitted by Tier 1.
#[cfg(not(target_arch = "wasm32"))]
fn fold_deep_outcome(
    outcome: Result<(), DeepPassError>,
    start: usize,
    origin: &str,
    report: &mut Report,
) {
    match outcome {
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
        Err(DeepPassError::NativeGates(msg)) => {
            let mut finding = Finding::new(
                Severity::Error,
                crate::codes::VALIDATE_DEEP_CONTRACT_INVALID,
                format!("deep semantic pass: required native verification laws are invalid: {msg}"),
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
/// user's worlds. Both datasets are borrowed from the caller's already parsed
/// request; the deep pass performs no bundle import or user-data parse.
///
/// # Errors
///
/// Returns [`DeepPassError::Unavailable`] if native projection, reasoning or
/// materialization fails. Input parsing belongs to the caller's request boundary.
///
/// Returns [`DeepPassError::ContractResolution`] if the bundle's declared
/// `logic:ReasoningContract` carries a garbled `logic:admissibleValuation` that
/// cannot be resolved to a [`gmeow_logic::certificate::ContradictionPolicy`]. This is INVALID INPUT and
/// must HARD-FAIL the gate; the caller emits a `Severity::Error` finding.
///
/// Returns [`DeepPassError::NativeGates`] if the required compact native law
/// member cannot be decoded or its source identity is invalid. This also
/// hard-fails the gate rather than omitting mathematical verification.
#[cfg(not(target_arch = "wasm32"))]
fn deep_consistency_findings(
    native_gates: Option<gmeow_errors::Result<&PreparedReasonedGates>>,
    bundle: &RdfDataset,
    user: &RdfDataset,
    report: &mut Report,
) -> Result<(), DeepPassError> {
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
    let policy = gmeow_logic::certificate::ContradictionPolicy::resolve_from_dataset(bundle)
        .map_err(|e| {
            DeepPassError::ContractResolution(format!("contract resolution failed: {e}"))
        })?;

    // Admit selected laws before execution so a known contract error cannot be
    // masked by an earlier infrastructure or resource failure.
    let gates = native_gates
        .ok_or_else(|| {
            DeepPassError::NativeGates("native verification laws were not selected".to_owned())
        })?
        .map_err(|error| DeepPassError::NativeGates(error.to_string()))?;
    let verification = gmeow_logic::verify::PreparedVerification::new(&[], gates)
        .map_err(|error| DeepPassError::NativeGates(error.to_string()))?;

    // Narrow the bundle side to the object-level reasoning EDB — the SAME
    // boundary `crates/pipeline`'s `assemble_object_level_edb` / `stage-reason` use at
    // build time (shared via `gmeow_logic::reasoning_graphs::project_object_level_edb`)
    // — BEFORE merging in the caller's own data, so `gmeow validate <data> --deep`
    // reasons the consumer's data against byte-identical bundle worlds to the
    // pipeline's own `make reason-verify` gate rather than also reasoning over
    // meta/report graphs (documentation, diagnostics, correspondence, …) that assert
    // no object-level axioms.
    let bundle_edb =
        gmeow_logic::reasoning_graphs::project_object_level_edb(bundle).map_err(|e| {
            DeepPassError::Unavailable(format!("object-level EDB projection failed: {e}"))
        })?;
    let edb = {
        let mut builder = purrdf::RdfDatasetBuilder::new();
        builder.push_dataset(bundle_edb.as_ref());
        builder.push_dataset(user);
        builder
            .freeze()
            .map_err(|e| DeepPassError::Unavailable(format!("freeze merged EDB: {e}")))?
    };
    let result = (|| {
        use gmeow_logic::reason::{
            DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld,
        };
        let input = gmeow_logic::reason::prepare_reasoning_input(&edb)?;
        let mut roles = gmeow_logic::reasoning_graphs::object_level_domains()?
            .worlds()
            .to_vec();
        // This operation admits every submitted user context as a theory. The
        // bundle side retains only its separately declared object-level roles.
        for graph in user
            .named_graphs()
            .map(|id| LogicalGraph::Named(user.term_value(id)))
        {
            if !roles.iter().any(|role| role.graph() == &graph) {
                roles.push(SelectedLogicalWorld::new(
                    graph,
                    DomainProfile::NonemptyObjectDomainV1,
                    "gmeow.validate.user-theories.v1".to_owned(),
                    *input.ingress_contract(),
                )?);
            }
        }
        gmeow_logic::reason::reason_all(input, &SelectedDomains::new(roles)?)
    })()
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
    crate::validate_all::fold_reasoning_result(&result, policy, &explanations, report)
        .map_err(|e| DeepPassError::Derivation(e.message))?;

    // Shared with `crate::validate_all::deep_semantic_findings` via
    // `crate::validate_all::run_math_reasoned_gates` (see its doc comment for why
    // this is safe to run unconditionally over the CALLER'S OWN data merged with
    // the bundle, and why the two callers deliberately map its failure
    // differently). Runs over the SAME `edb` + `result` the consistency fold above
    // just used. Unlike the dev bundle-only pass, a failure here can be caused by
    // the CALLER's own merged data, not just the bundle, so it degrades
    // gracefully to the `Unavailable` advisory rather than hard-failing.
    crate::validate_all::run_math_reasoned_gates(edb.as_ref(), &result, &verification, report)
        .map_err(|e| {
            DeepPassError::Unavailable(format!("reasoned-graph materialization failed: {e}"))
        })?;
    Ok(())
}

/// Parse external RDF data bytes into a graph-preserving [`RdfDataset`] for the
/// Tier-2 reasoner (the world structure must survive, so this does NOT flatten the
/// way [`data_store`] does for SHACL). Handles every supported format, routing
/// JSON-LD through the gmeow-gts codec exactly as [`data_store`] does.
fn data_dataset(data_bytes: &[u8], data_format: &str) -> gmeow_errors::Result<Arc<RdfDataset>> {
    if is_json_ld(data_format) {
        // JSON-LD has no native-codec media type; route it through the FIRST-PARTY
        // native JSON-LD-star codec, which folds the RDF 1.2 statement layer and
        // PRESERVES named graphs — the graph-preserving shape this Tier-2 path needs
        // (no longer the external gmeow-gts JSON-LD codec).
        return purrdf::native_codecs::jsonld::parse_jsonld(data_bytes, None).map_err(|e| {
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
    flatten_to_default_graph(&data_dataset(data_bytes, data_format)?)
}

/// Derive the flat validation view, retaining base quads, reifiers and annotations.
/// World-scoped rows deliberately join in this SHACL view; the original dataset
/// remains authoritative for deep reasoning and provenance. An already flat input
/// is shared directly, and an invalid native projection fails at freeze.
pub(crate) fn flatten_to_default_graph(
    dataset: &Arc<RdfDataset>,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    if dataset.named_graphs().next().is_none() {
        return Ok(Arc::clone(dataset));
    }
    use purrdf::RdfDatasetBuilder;
    let mut builder = RdfDatasetBuilder::new();
    for mut quad in dataset.owned_quads() {
        quad.graph_name = None;
        builder.push_owned_quad(&quad);
    }
    for mut reifier in dataset.owned_reifiers() {
        reifier.graph = None;
        builder.push_owned_reifier(&reifier);
    }
    for mut annotation in dataset.owned_annotations() {
        annotation.graph = None;
        builder.push_owned_annotation(&annotation);
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
/// Returns `Err` if the native merge fails to freeze. Shortcut terms come directly
/// from the accepted ontology; no intermediate Turtle serialization is produced.
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

    let mut shortcuts = Vec::new();
    for class_iri in &used_types {
        let mut ancestors: Vec<String> = gufo::proper_ancestors(ontology, class_iri)
            .into_iter()
            .collect();
        ancestors.sort();
        for ancestor in ancestors {
            shortcuts.push(purrdf::RdfQuad::new(
                purrdf::RdfTerm::iri(class_iri),
                gmeow_ns::RDFS_SUB_CLASS_OF,
                purrdf::RdfTerm::iri(ancestor),
            ));
        }
    }
    if shortcuts.is_empty() {
        return Ok(dataset);
    }

    let mut builder = purrdf::RdfDatasetBuilder::new();
    builder.push_dataset(&dataset);
    for shortcut in shortcuts {
        builder.push_owned_quad(&shortcut);
    }
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

/// Import the dataset and exactly the archives required by this validation profile.
fn import_validation_bundle(
    bytes: &[u8],
    deep: bool,
) -> gmeow_errors::Result<purrdf::GtsImportWithBlobs> {
    use purrdf::GtsBlobSelector::Representation;
    let mut selected = vec![Representation(REP_SHAPES)];
    if deep {
        selected.push(Representation(gmeow_gts_profile::archive::REASONING_REP));
    }
    let limit = gmeow_gts_profile::archive::MAX_SELECTED_ARCHIVE_BYTES;
    purrdf::import_gts_events_with_blobs(bytes, &selected, purrdf::GtsBlobLimits::new(limit, limit))
        .map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::Dataset {
                detail: format!("import selected validation bundle: {error}"),
            })
        })
}

/// Assemble the data-graph shape union from an already authenticated archive.
/// The native archive iterator owns format decoding; only selected Turtle text
/// is copied into the parser's one document, in deterministic member order.
///
/// # Errors
/// Rejects upstream archive errors, invalid shape text and an empty selection.
pub fn data_graph_shapes_from_archive(bytes: &[u8]) -> gmeow_errors::Result<String> {
    let mut rows = archive_shape_rows(bytes)?;
    rows.sort_by(|left, right| left.0.cmp(right.0));
    assemble_shape_rows(rows, &[])
}

/// Extract and assemble the data-graph SHACL shape union (one Turtle document)
/// from the bundle's `shapes-archive` blob.
pub fn data_graph_shapes_from_gts(gts_bytes: &[u8]) -> gmeow_errors::Result<String> {
    shapes_from_gts_excluding(gts_bytes, &[])
}

/// Three authenticated shape selections consumed by the test corpus.
///
/// The explicit producer derives all three from one decoded `shapes-archive`; test
/// processes load the published byte artifacts and never call this producer seam.
pub struct ShapeCorpusVariants {
    /// Complete data-graph shape surface used by production validation.
    pub production: String,
    /// Fixture-conformance surface, excluding `validation-shapes.ttl`.
    pub conformance: String,
    /// Domain-conformance surface, excluding `result-shapes.ttl`.
    pub domain_conformance: String,
}

/// Decode the bundle and its shapes archive once, then derive every authenticated
/// test-corpus shape selection from that single member table.
pub fn shape_corpus_variants_from_gts(
    gts_bytes: &[u8],
) -> gmeow_errors::Result<ShapeCorpusVariants> {
    let imported = import_validation_bundle(gts_bytes, false)?;
    let blob = gmeow_gts_profile::archive::required_imported_blob(&imported, REP_SHAPES)?;
    shape_corpus_variants_from_archive(&blob.bytes)
}

/// Derive every producer-selected shape surface from borrowed archive members.
/// No member body is copied before the three required output documents are built.
///
/// # Errors
/// Propagates archive decoding, UTF-8 and empty-selection failures.
pub fn shape_corpus_variants_from_archive(
    bytes: &[u8],
) -> gmeow_errors::Result<ShapeCorpusVariants> {
    shape_corpus_variants_from_rows(archive_shape_rows(bytes)?)
}

fn archive_shape_rows(bytes: &[u8]) -> gmeow_errors::Result<Vec<(&str, &[u8])>> {
    purrdf::ustar::archive_members(bytes)
        .map(|member| member.map(|member| (member.name, member.data)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|detail| gmeow_errors::Diag::of_kind(crate::error::Dataset { detail }))
}

/// Derive the selected shape surfaces from an already decoded archive, sharing
/// the caller's bundle view and archive work across all fixture artifacts.
pub fn shape_corpus_variants_from_members(
    members: &[(String, Vec<u8>)],
) -> gmeow_errors::Result<ShapeCorpusVariants> {
    shape_corpus_variants_from_rows(
        members
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect(),
    )
}

fn shape_corpus_variants_from_rows(
    mut members: Vec<(&str, &[u8])>,
) -> gmeow_errors::Result<ShapeCorpusVariants> {
    members.sort_by(|left, right| left.0.cmp(right.0));
    Ok(ShapeCorpusVariants {
        production: assemble_shape_rows(members.iter().copied(), &[])?,
        conformance: assemble_shape_rows(members.iter().copied(), &["validation-shapes.ttl"])?,
        domain_conformance: assemble_shape_rows(members.iter().copied(), &["result-shapes.ttl"])?,
    })
}

/// Extract the bundle-carried SHACL union while excluding additional member basenames.
///
/// The production Tier-1 surface calls [`data_graph_shapes_from_gts`] with no additional
/// exclusions. The explicit test-corpus producer uses this to retain the historical
/// fixture-conformance exclusion of `validation-shapes.ttl`; test processes consume its
/// authenticated output and never reassemble the archive themselves.
pub fn shapes_from_gts_excluding(
    gts_bytes: &[u8],
    additional_excluded: &[&str],
) -> gmeow_errors::Result<String> {
    let imported = import_validation_bundle(gts_bytes, false)?;
    let blob = gmeow_gts_profile::archive::required_imported_blob(&imported, REP_SHAPES)?;
    let mut rows = archive_shape_rows(&blob.bytes)?;
    rows.sort_by(|left, right| left.0.cmp(right.0));
    assemble_shape_rows(rows, additional_excluded)
}

fn assemble_shape_rows<'a>(
    members: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    additional_excluded: &[&str],
) -> gmeow_errors::Result<String> {
    let mut ttl = String::new();
    let mut included = 0usize;
    for (name, bytes) in members {
        if !name.ends_with(".ttl") {
            continue;
        }
        let base = name.rsplit('/').next().unwrap_or(name);
        if EXCLUDED.contains(&base) || additional_excluded.contains(&base) {
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

// The deep-pass tests exercise `run_deep_pass`, which is native-only; the whole
// module is gated to the native target so a wasm `--all-targets` pass stays clean.
#[path = "data_validate.tests.rs"]
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
