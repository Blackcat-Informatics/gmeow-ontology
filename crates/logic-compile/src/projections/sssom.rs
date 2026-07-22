// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SSSOM correspondence lowering: `gmeow:TermEquivalence` cells → SSSOM TSV.
//!
//! SSSOM is the 1:1-lattice-band lowering of the correspondence calculus. Each
//! `gmeow:TermEquivalence` compiles to exactly one SSSOM row; the target drops the
//! caveat/law/leg structure of a full correspondence (it carries only the
//! subject/predicate/object, a confidence, and a justification), so its ledger-row
//! preservation is `SoundUnder`.
//!
//! Extraction (deriving [`SssomMapping`] rows and per-file [`MappingSet`] header
//! metadata from the DSL) is GMEOW's own; SERIALIZATION is `purrdf::sssom::serialize_tsv`
//! (the canonical PurRDF SSSOM TSV codec — YAML-ish `#` header with `curie_map`, dynamic
//! column set, rows sorted by `(subject_id, predicate_id, object_id)`). The one GMEOW
//! convention `purrdf`'s codec has no slot for — a refused/deferred-mapping provenance
//! trailer folded in as `# #` comments — is spliced into the canonical output afterwards
//! ([`splice_trailer`]), so no content is lost even though the row/header serialization
//! itself is no longer bespoke. Extraction runs over the oxigraph-free [`DslView`]; the
//! version/date come from the caller (which reads `metadata/gmeow-self.ttl`).

use std::collections::{BTreeMap, BTreeSet};

use purrdf::{SssomMapping, SssomMappingSet, SssomMeta};

use crate::ingest::DslView;
use crate::ingest::prefixes::{ns_to_prefix, registry_iri, sssom_id};
use crate::ir::{CorrespondenceRelation, MorphismClass};
use crate::projections::correspondence_frontend::CorrespondenceLookup;
use crate::projections::correspondence_gate::assert_relation_no_overclaim;
use crate::projections::get_leg::{MappingPattern, ProfileBinding, ProjectionCell, projections};
use crate::projections::{ProjectionResult, correspondence_result};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const GM_TERM_EQUIVALENCE: &str = "https://blackcatinformatics.ca/gmeow/TermEquivalence";
const GM_ALIGN_SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/alignSubject";
const GM_ALIGN_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/alignPredicate";
const GM_ALIGN_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/alignObject";
const GM_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const GM_JUSTIFICATION: &str = "https://blackcatinformatics.ca/gmeow/justification";
const GM_COMMENT: &str = "https://blackcatinformatics.ca/gmeow/comment";
const GM_LOSSY_DROP: &str = "https://blackcatinformatics.ca/gmeow/lossyDrop";
const GM_SSSOM_FILE: &str = "https://blackcatinformatics.ca/gmeow/sssomFile";
const GM_SUBJECT_LABEL: &str = "https://blackcatinformatics.ca/gmeow/subjectLabel";
const GM_OBJECT_LABEL: &str = "https://blackcatinformatics.ca/gmeow/objectLabel";
const LOGIC_GROUNDING_CORRESPONDENCE: &str =
    "https://blackcatinformatics.ca/logic/GroundingCorrespondence";
const LOGIC_MORPHISM_CLASS: &str = "https://blackcatinformatics.ca/logic/morphismClass";
const LOGIC_MORPHISM_KIND: &str = "https://blackcatinformatics.ca/logic/morphismKind";
const LOGIC_PRESERVATION_KIND: &str = "https://blackcatinformatics.ca/logic/preservationKind";
const LOGIC_SOURCE_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/sourceEndpoint";
const LOGIC_TARGET_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/targetEndpoint";
const GM_MAPPING_SET: &str = "https://blackcatinformatics.ca/gmeow/MappingSet";
const GM_SET_ID: &str = "https://blackcatinformatics.ca/gmeow/setId";
const GM_LICENSE: &str = "https://blackcatinformatics.ca/gmeow/license";
const GM_SET_COMMENT: &str = "https://blackcatinformatics.ca/gmeow/setComment";
const GM_SET_TRAILER: &str = "https://blackcatinformatics.ca/gmeow/setTrailer";

const DEFAULT_JUSTIFICATION: &str = "https://w3id.org/semapv/vocab/ManualMappingCuration";

/// One `gmeow:TermEquivalence` cell — compiles to exactly one SSSOM row. IRIs are
/// kept full and absolute; CURIE-shortening happens at render time.
///
/// `pub(crate)` (with the frontend-relevant fields exposed) so the
/// `logic:Correspondence` transpiler materializes one typed node per cell from THE SAME
/// extraction the SSSOM renderer reads — no second, drifting read of the store.
#[derive(Debug, Clone)]
pub struct EquivalenceCell {
    pub subject: String,
    pub predicate: String,
    pub obj: String,
    pub confidence: Option<f64>,
    pub(crate) justification: Option<String>,
    /// Optional authored law-spine rung. Absent cells retain the predicate-derived SSSOM
    /// default; grounding bridges author this explicitly so a commitment shift can never
    /// be flattened into an ordinary lens.
    pub(crate) morphism_class: Option<String>,
    /// Optional authored satisfaction/commitment qualifier.
    pub(crate) morphism_kind: Option<String>,
    /// Optional authored per-correspondence preservation judgment.
    pub(crate) preservation: Option<String>,
    /// Explicit source endpoint of a grounding correspondence. Ordinary SSSOM cells may
    /// omit it; grounding cells must carry it and it must agree with `alignSubject`.
    pub(crate) source_endpoint: Option<String>,
    /// Explicit target endpoint of a grounding correspondence. Ordinary SSSOM cells may
    /// omit it; grounding cells must carry it and it must agree with `alignObject`.
    pub(crate) target_endpoint: Option<String>,
    /// Whether the frontend cell is explicitly a `logic:GroundingCorrespondence`.
    pub(crate) grounding: bool,
    comment: String,
    /// Structured per-correspondence drop notes (`gmeow:lossyDrop`) — the specific
    /// constructs this by-reference lowering does not carry (e.g. a loop unrolls, a
    /// concurrent composition serializes, a per-outcome compensation is omitted). Folded
    /// into the report's per-correspondence residue, distinct from the human `comment`.
    lossy_drops: Vec<String>,
    sssom_file: String,
    subject_label: String,
    pub object_label: String,
}

/// Per-file SSSOM header metadata (`gmeow:MappingSet`).
#[derive(Debug, Clone, Default)]
struct MappingSet {
    set_id: String,
    license: String,
    comment: String,
    trailer: String,
}

/// Curated SSSOM-facing metadata carried by a `gmeow:ProjectionMapping` itself.
/// Keeping this beside the parsed get-leg model lets a correspondence replace a legacy
/// `TermEquivalence` without discarding its evidence, labels, or explanatory note.
#[derive(Debug, Clone, Default)]
struct ProjectionSssomMetadata {
    justification: Option<String>,
    comment: String,
    subject_label: String,
    object_label: String,
}

type RowsByFile = BTreeMap<String, Vec<SssomMapping>>;
type PreservationLedger = Vec<ProjectionResult>;

/// The discovered SSSOM source model: every equivalence cell and the per-file
/// mapping-set metadata.
struct SssomSources {
    equivalences: Vec<EquivalenceCell>,
    projections: Vec<ProjectionCell>,
    projection_metadata: BTreeMap<String, ProjectionSssomMetadata>,
    mapping_sets: BTreeMap<String, MappingSet>,
}

/// The artifacts + per-correspondence loss ledger of the SSSOM lowering.
pub struct SssomLowering {
    /// Bare file name (e.g. `gmeow-accessibility.sssom.tsv`) → TSV.
    pub sets: BTreeMap<String, String>,
    /// One [`ProjectionResult`] per `gmeow:TermEquivalence` correspondence — SSSOM
    /// always drops the caveat/law/leg structure and world/standpoint scope, so every
    /// cell contributes a preservation row.
    pub ledger: Vec<ProjectionResult>,
    /// The per-correspondence loss store this lowering interned every drop into (keyed by
    /// target focus). The mappings stage unions it into the single report loss store so the
    /// SSSOM rows' `gmeow:lossyDrop` records read back from the SAME substrate ledger.
    pub loss: crate::loss_ledger::LossLedger,
}

/// Lower every `gmeow:TermEquivalence` in `view` to its SSSOM TSV, keyed by bare file
/// name (e.g. `gmeow-accessibility.sssom.tsv`), plus the per-correspondence loss
/// ledger. `version`/`release_date` come from the caller's read of
/// `metadata/gmeow-self.ttl`.
///
/// # Errors
///
/// Returns the overclaim message if a cell emits an equivalence predicate
/// (`exactMatch`/`equivalentClass`/`equivalentProperty`) that the SSSOM predicate
/// lattice does not classify as a genuine `logic:Equiv` (Constitution Principle 5).
pub fn lower_sssom(
    view: &DslView,
    version: &str,
    release_date: &str,
    lookup: &CorrespondenceLookup,
) -> gmeow_errors::Result<SssomLowering> {
    let mut loss = crate::loss_ledger::LossLedger::new();
    let sources = collect_sources(view)?;
    let (rows_by_file, ledger) = build_rows_and_ledger(&sources, lookup, &mut loss)?;
    let sets = render_sets(&sources.mapping_sets, &rows_by_file, version, release_date);
    Ok(SssomLowering { sets, ledger, loss })
}

/// Map an SSSOM mapping predicate to the typed `logic:` correspondence relation it
/// asserts. The predicate IS the relation for the 1:1 lattice band; this lets the
/// overclaim gate refuse, e.g., a `relatedMatch`-classed predicate masquerading as an
/// `exactMatch` token were the two ever to disagree.
///
/// `pub(crate)` so the `logic:Correspondence` frontend transpiler
/// ([`crate::projections::correspondence_frontend`]) materializes its typed relation from
/// THE SAME logic the SSSOM ledger gate uses — one derivation, never a fork.
pub(crate) fn sssom_relation(predicate: &str) -> CorrespondenceRelation {
    let local = predicate
        .rsplit(['#', '/', ':'])
        .next()
        .unwrap_or(predicate);
    match local {
        "exactMatch" | "equivalentClass" | "equivalentProperty" | "sameAs" => {
            CorrespondenceRelation::Equiv
        }
        "broadMatch" | "subClassOf" | "subPropertyOf" => CorrespondenceRelation::Subsumes,
        "narrowMatch" => CorrespondenceRelation::SubsumedBy,
        "closeMatch" => CorrespondenceRelation::Overlaps,
        _ => CorrespondenceRelation::RelatedMatch,
    }
}

/// The `(relation, morphism class)` band an SSSOM 1:1 cell occupies, given its align
/// predicate. The SSSOM band is a satisfaction-preserving lens, never a bridge: the
/// morphism class is the strongest rung the relation can lawfully claim (an honest
/// under-approximation — composition can only weaken it).
///
/// `pub(crate)` so the correspondence frontend transpiler and the SSSOM ledger gate
/// derive the band identically (DRY: the single mapping `predicate → (relation, class)`).
pub(crate) fn sssom_band(predicate: &str) -> (CorrespondenceRelation, MorphismClass) {
    let relation = sssom_relation(predicate);
    let mclass = match relation {
        CorrespondenceRelation::Equiv => MorphismClass::WellBehavedLens,
        CorrespondenceRelation::Subsumes | CorrespondenceRelation::SubsumedBy => {
            MorphismClass::LossyLens
        }
        CorrespondenceRelation::Overlaps => MorphismClass::AffineCorrespondence,
        _ => MorphismClass::AffineCorrespondence,
    };
    (relation, mclass)
}

/// Build one preservation row per `gmeow:TermEquivalence` correspondence, running the
/// overclaim gate over each emitted predicate. The typed `(relation, morphism class,
/// morphism kind)` is CONSUMED from the materialized correspondence set (`lookup`) — the
/// single source of truth — not re-derived inline here (F5 Task 2).
fn build_rows_and_ledger(
    sources: &SssomSources,
    lookup: &CorrespondenceLookup,
    loss: &mut crate::loss_ledger::LossLedger,
) -> gmeow_errors::Result<(RowsByFile, PreservationLedger)> {
    let table = ns_to_prefix();
    let mut by_file: RowsByFile = BTreeMap::new();
    let mut ledger: PreservationLedger = Vec::new();
    for cell in &sources.equivalences {
        // Consume the typed relation/class/kind from the materialized correspondence keyed
        // by this cell's natural identity (subject, predicate, object). A miss is a HARD
        // FAIL — every authored cell is transpiled (no-optionality).
        let typed = lookup.equivalence(&cell.subject, &cell.predicate, &cell.obj)?;
        assert_relation_no_overclaim(
            "sssom",
            typed.relation,
            typed.morphism_class,
            typed.morphism_kind,
            &cell.predicate,
        )
        .map_err(|e| gmeow_errors::Diag::of_kind(crate::error::Sssom { detail: e.0 }))?;

        // SSSOM carries only subject/predicate/object + confidence + justification; the
        // correspondence's caveat/law/leg structure and world/standpoint scope are
        // dropped (the dialect structural drops, attributed to the get leg).
        let mut residue = vec![
            "get-leg: the caveat/law/leg structure of the correspondence is dropped \
             (only subject/predicate/object, confidence, and justification survive)"
                .to_owned(),
            "get-leg: world/standpoint scope and the put leg are not carried by SSSOM".to_owned(),
        ];
        // Author-declared per-correspondence drops (gmeow:lossyDrop) — the specific
        // constructs a by-reference engine surface cannot carry (a loop unrolls/errors, a
        // concurrent composition serializes, a per-outcome compensation is omitted) — are
        // structured residue notes, so the loss ledger records WHAT each lowering drops
        // rather than leaving it to prose.
        residue.extend(cell.lossy_drops.iter().cloned());
        // A correspondence is the (subject, predicate, object) triple, not just the
        // subject (one subject may align to several objects), so the per-correspondence
        // key folds all three for a stable, collision-free target name.
        let key = format!("{}|{}|{}", cell.subject, cell.predicate, cell.obj);
        ledger.push(correspondence_result(
            loss,
            "sssom",
            &key,
            residue,
            crate::projections::gmeow_endpoint(&cell.subject, &cell.obj),
        ));

        let justification = cell
            .justification
            .clone()
            .unwrap_or_else(|| DEFAULT_JUSTIFICATION.to_owned());
        by_file
            .entry(cell.sssom_file.clone())
            .or_default()
            .push(checked_mapping(
                sssom_id(&cell.subject, table),
                opt(cell.subject_label.clone()),
                sssom_id(&cell.predicate, table),
                sssom_id(&cell.obj, table),
                opt(cell.object_label.clone()),
                sssom_id(&justification, table),
                cell.confidence,
                opt(cell.comment.clone()),
            )?);
    }

    for cell in &sources.projections {
        let metadata = sources
            .projection_metadata
            .get(&cell.iri)
            .expect("every parsed projection has extracted SSSOM metadata");
        for binding in &cell.bindings {
            if !binding.emit_sssom {
                continue;
            }
            let predicate = binding.sssom_predicate.as_deref().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Sssom {
                    detail: format!(
                        "projection binding {}::{} has gmeow:emitSssom true but no gmeow:sssomPredicate",
                        cell.iri, binding.profile
                    ),
                })
            })?;
            let file = binding.sssom_file.as_deref().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Sssom {
                    detail: format!(
                        "projection binding {}::{} has gmeow:emitSssom true but no gmeow:sssomFile",
                        cell.iri, binding.profile
                    ),
                })
            })?;

            let typed = lookup.binding(&cell.iri, &binding.profile)?;
            assert_relation_no_overclaim(
                "sssom",
                typed.relation,
                typed.morphism_class,
                typed.morphism_kind,
                predicate,
            )
            .map_err(|e| gmeow_errors::Diag::of_kind(crate::error::Sssom { detail: e.0 }))?;

            let pairs = projection_sssom_pairs(cell, binding)?;
            for (subject, obj) in pairs {
                by_file
                    .entry(file.to_owned())
                    .or_default()
                    .push(checked_mapping(
                        sssom_id(&subject, table),
                        opt(metadata.subject_label.clone()),
                        sssom_id(predicate, table),
                        sssom_id(&obj, table),
                        opt(metadata.object_label.clone()),
                        sssom_id(
                            metadata
                                .justification
                                .as_deref()
                                .unwrap_or(DEFAULT_JUSTIFICATION),
                            table,
                        ),
                        binding.confidence,
                        opt(metadata.comment.clone()),
                    )?);
            }

            let mut residue = vec![
                "get-leg: the projection pattern, guards, transforms, executable branch, and \
                 EDOAL path structure are dropped (only subject/predicate/object, confidence, \
                 and justification survive)"
                    .to_owned(),
                "get-leg: world/standpoint scope and the put leg are not carried by SSSOM"
                    .to_owned(),
            ];
            residue.extend(
                binding
                    .lossy_drops
                    .iter()
                    .map(|d| format!("get-leg profile loss: {d}")),
            );
            let key = format!("{}::{}", local_name(&cell.iri), binding.profile);
            // Profile-binding cells are gmeow:ProjectionMapping views (no clean
            // subject/object IRI pair); their residue stays whole-program.
            ledger.push(correspondence_result(loss, "sssom", &key, residue, None));
        }
    }
    Ok((by_file, ledger))
}

/// Every IRI participating in an SSSOM equivalence (both subject and object position)
/// — the alignment-terms set the projection lints consume.
pub fn alignment_terms(view: &DslView) -> BTreeSet<String> {
    let Ok(sources) = collect_sources(view) else {
        return BTreeSet::new();
    };
    let mut terms = BTreeSet::new();
    for cell in &sources.equivalences {
        terms.insert(cell.subject.clone());
        terms.insert(cell.obj.clone());
    }
    for cell in &sources.projections {
        for binding in &cell.bindings {
            if !binding.emit_sssom {
                continue;
            }
            if let Ok(pairs) = projection_sssom_pairs(cell, binding) {
                for (subject, obj) in pairs {
                    terms.insert(subject);
                    terms.insert(obj);
                }
            }
        }
    }
    terms
}

// ── Extraction (over the oxigraph-free DslView) ──────────────────────────────────

/// Every `gmeow:TermEquivalence` cell discovered over `view`, in extraction order — the
/// frontend transpiler's input. Shares [`extract_equivalences`] with the SSSOM renderer,
/// so the typed correspondence set and the rendered TSV read the store identically.
pub fn equivalence_cells(view: &DslView) -> Vec<EquivalenceCell> {
    let mut out = Vec::new();
    extract_equivalences(view, &mut out);
    out
}

fn collect_sources(view: &DslView) -> gmeow_errors::Result<SssomSources> {
    let mut equivalences = Vec::new();
    let mut mapping_sets = BTreeMap::new();
    extract_equivalences(view, &mut equivalences);
    extract_mapping_sets(view, &mut mapping_sets);
    let projections = projections(view)?;
    let projection_metadata = projections
        .iter()
        .map(|cell| {
            (
                cell.iri.clone(),
                ProjectionSssomMetadata {
                    justification: view.object_iri(&cell.iri, GM_JUSTIFICATION),
                    comment: view
                        .object_literal(&cell.iri, GM_COMMENT)
                        .unwrap_or_default(),
                    subject_label: view
                        .object_literal(&cell.iri, GM_SUBJECT_LABEL)
                        .unwrap_or_default(),
                    object_label: view
                        .object_literal(&cell.iri, GM_OBJECT_LABEL)
                        .unwrap_or_default(),
                },
            )
        })
        .collect();
    Ok(SssomSources {
        equivalences,
        projections,
        projection_metadata,
        mapping_sets,
    })
}

fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

fn projection_sssom_pairs(
    cell: &ProjectionCell,
    binding: &ProfileBinding,
) -> gmeow_errors::Result<Vec<(String, String)>> {
    if !binding.value_class_map.is_empty() {
        return Ok(binding
            .value_class_map
            .iter()
            .map(|entry| (entry.when_value.clone(), entry.to_class.clone()))
            .collect());
    }

    let source = projection_sssom_subject(cell, binding)?;
    let target = projection_sssom_object(cell, binding)?;
    Ok(vec![(source, target)])
}

fn projection_sssom_subject(
    cell: &ProjectionCell,
    binding: &ProfileBinding,
) -> gmeow_errors::Result<String> {
    if let Some(source) = &cell.pattern.edoal_source {
        return Ok(source.clone());
    }

    let source_values = fixed_pattern_values(&cell.pattern);
    let target_values = fixed_template_values(binding);
    if source_values.len() == 1 && target_values.len() == 1 {
        return Ok(source_values[0].clone());
    }
    Err(gmeow_errors::Diag::of_kind(crate::error::Sssom {
        detail: format!(
            "cannot derive SSSOM subject for projection binding {}::{}; author gmeow:edoalSource \
             or use an unambiguous fixed value rewrite",
            cell.iri, binding.profile
        ),
    }))
}

fn projection_sssom_object(
    cell: &ProjectionCell,
    binding: &ProfileBinding,
) -> gmeow_errors::Result<String> {
    if let Some(target) = binding
        .to_predicate
        .as_ref()
        .or(binding.to_class.as_ref())
        .or(binding.edoal_target.as_ref())
    {
        return Ok(target.clone());
    }

    let source_values = fixed_pattern_values(&cell.pattern);
    let target_values = fixed_template_values(binding);
    if source_values.len() == 1 && target_values.len() == 1 {
        return Ok(target_values[0].clone());
    }
    Err(gmeow_errors::Diag::of_kind(crate::error::Sssom {
        detail: format!(
            "cannot derive SSSOM object for projection binding {}::{}; author gmeow:toPredicate, \
             gmeow:toClass, gmeow:edoalTarget, gmeow:valueClassMap, or use an unambiguous fixed \
             value rewrite",
            cell.iri, binding.profile
        ),
    }))
}

fn fixed_pattern_values(pattern: &MappingPattern) -> Vec<String> {
    pattern
        .flat_atoms()
        .into_iter()
        .filter_map(|atom| atom.object_value)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn fixed_template_values(binding: &ProfileBinding) -> Vec<String> {
    binding
        .template_atoms
        .iter()
        .filter_map(|atom| atom.object_value.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The five `skos:*Match` predicate local-names an RDF-1.2 alignment cell may carry. A
/// reified statement whose predicate is one of these AND whose annotation block carries
/// the `gmeow:sssomFile` discriminator is an alignment cell; anything else (a bare
/// `skos:exactMatch` A-Box coreference with no reifier/annotation) is NOT.
/// Whether a reified statement's predicate is one the alignment lattice classifies — the
/// full set the SSSOM band ([`sssom_relation`]) recognizes, NOT only the five `skos:*Match`
/// names. The authored `gmeow:alignPredicate` surface carried OWL/RDFS alignment predicates
/// too (`owl:equivalentClass`/`equivalentProperty`, `rdfs:subClassOf`/`subPropertyOf`,
/// `owl:sameAs`), so the native reader must accept them as well or the greenfield migration
/// could never delete the align* path (those cells would have no native home). Note: an
/// alignment cell lives ONLY in a Mapping-role file (`slices/**/mappings/`), which the
/// object-level authored graph (`source_load::authored_files`) never loads, so an asserted
/// `owl:equivalentClass` here is alignment metadata that never enters OWL closure — and the
/// `gmeow:sssomFile` annotation remains the authoritative discriminator on top of this check.
fn is_alignment_predicate(predicate: &str) -> bool {
    let local = predicate
        .rsplit(['#', '/', ':'])
        .next()
        .unwrap_or(predicate);
    matches!(
        local,
        "exactMatch"
            | "closeMatch"
            | "broadMatch"
            | "narrowMatch"
            | "relatedMatch"
            | "equivalentClass"
            | "equivalentProperty"
            | "sameAs"
            | "subClassOf"
            | "subPropertyOf"
    )
}

/// Read every native-form alignment cell: one RDF-1.2 asserting-annotation on a
/// `skos:*Match` triple whose reifier carries the `gmeow:sssomFile` discriminator. This
/// is the PRIMARY (canonical native) reader; [`extract_equivalences`] also runs the
/// legacy `gmeow:align*` cell reader and both feed the SAME `EquivalenceCell` vec.
///
/// The discriminator is load-bearing: a bare `ex:x skos:exactMatch ex:y` with no reifier
/// (instance coreference — `examples/authority-links.ttl`, `music/fixtures/*`) has no
/// reified statement here at all, and a reified `skos:*Match` without `gmeow:sssomFile` is
/// skipped, so A-Box coreference is never swept into the alignment corpus.
fn extract_native_equivalences(view: &DslView, out: &mut Vec<EquivalenceCell>) {
    for stmt in view.reified_statements() {
        if !is_alignment_predicate(&stmt.predicate) {
            continue;
        }
        // The discriminator: an alignment cell MUST annotate its match triple with
        // gmeow:sssomFile. Absent → not an alignment cell (skip).
        let Some(sssom_file) = view.annotation_literal(&stmt.reifier, GM_SSSOM_FILE) else {
            continue;
        };
        // The match object must be an IRI (the align object); a literal/blank object is a
        // malformed cell, dropped silently here (the authoring gate rejects it upstream).
        let Some(object_iri_v) = stmt.object.as_iri().map(str::to_owned) else {
            continue;
        };
        let confidence = view
            .annotation_literal(&stmt.reifier, GM_CONFIDENCE)
            .and_then(|t| t.parse::<f64>().ok());
        out.push(EquivalenceCell {
            subject: stmt.subject.clone(),
            // The match triple's predicate IS the SSSOM/correspondence relation.
            predicate: stmt.predicate.clone(),
            obj: object_iri_v,
            confidence,
            justification: view.annotation_iri(&stmt.reifier, GM_JUSTIFICATION),
            morphism_class: view.annotation_iri(&stmt.reifier, LOGIC_MORPHISM_CLASS),
            morphism_kind: view.annotation_iri(&stmt.reifier, LOGIC_MORPHISM_KIND),
            preservation: view.annotation_iri(&stmt.reifier, LOGIC_PRESERVATION_KIND),
            source_endpoint: view.annotation_iri(&stmt.reifier, LOGIC_SOURCE_ENDPOINT),
            target_endpoint: view.annotation_iri(&stmt.reifier, LOGIC_TARGET_ENDPOINT),
            grounding: view.annotation_has_type(&stmt.reifier, LOGIC_GROUNDING_CORRESPONDENCE),
            comment: view
                .annotation_literal(&stmt.reifier, GM_COMMENT)
                .unwrap_or_default(),
            lossy_drops: view.annotation_literals(&stmt.reifier, GM_LOSSY_DROP),
            sssom_file,
            subject_label: view
                .annotation_literal(&stmt.reifier, GM_SUBJECT_LABEL)
                .unwrap_or_default(),
            object_label: view
                .annotation_literal(&stmt.reifier, GM_OBJECT_LABEL)
                .unwrap_or_default(),
        });
    }
}

fn extract_equivalences(view: &DslView, out: &mut Vec<EquivalenceCell>) {
    let _ = RDF_TYPE; // documented surface; subjects_of_type uses it internally.
    // The canonical native RDF-1.2 reader runs first (the primary path).
    extract_native_equivalences(view, out);
    let grounding: BTreeSet<String> = view
        .subjects_of_type(LOGIC_GROUNDING_CORRESPONDENCE)
        .into_iter()
        .collect();
    for subject in view.subjects_of_type(GM_TERM_EQUIVALENCE) {
        let (Some(subject_iri), Some(predicate_iri), Some(object_iri_v), Some(sssom_file)) = (
            view.object_iri(&subject, GM_ALIGN_SUBJECT),
            view.object_iri(&subject, GM_ALIGN_PREDICATE),
            view.object_iri(&subject, GM_ALIGN_OBJECT),
            view.object_literal(&subject, GM_SSSOM_FILE),
        ) else {
            // A malformed cell (missing subject/predicate/object/file) is dropped
            // silently here; the authoring SHACL gate rejects it upstream.
            continue;
        };
        let confidence = view
            .object_literal(&subject, GM_CONFIDENCE)
            .and_then(|t| t.parse::<f64>().ok());
        out.push(EquivalenceCell {
            subject: subject_iri,
            predicate: predicate_iri,
            obj: object_iri_v,
            confidence,
            justification: view.object_iri(&subject, GM_JUSTIFICATION),
            morphism_class: view.object_iri(&subject, LOGIC_MORPHISM_CLASS),
            morphism_kind: view.object_iri(&subject, LOGIC_MORPHISM_KIND),
            preservation: view.object_iri(&subject, LOGIC_PRESERVATION_KIND),
            source_endpoint: view.object_iri(&subject, LOGIC_SOURCE_ENDPOINT),
            target_endpoint: view.object_iri(&subject, LOGIC_TARGET_ENDPOINT),
            grounding: grounding.contains(&subject),
            comment: view
                .object_literal(&subject, GM_COMMENT)
                .unwrap_or_default(),
            lossy_drops: view.object_literals(&subject, GM_LOSSY_DROP),
            sssom_file,
            subject_label: view
                .object_literal(&subject, GM_SUBJECT_LABEL)
                .unwrap_or_default(),
            object_label: view
                .object_literal(&subject, GM_OBJECT_LABEL)
                .unwrap_or_default(),
        });
    }
}

fn extract_mapping_sets(view: &DslView, out: &mut BTreeMap<String, MappingSet>) {
    // Same-file collision: the lexically-smallest MappingSet IRI is canonical. The
    // `subjects_of_type` iteration is IRI-ascending and `or_insert` keeps the first,
    // so the smallest IRI wins — a deterministic rule replacing the historical store's
    // hash-order accident (e.g. gmeow-music declares both `gmeow:mapsetMusic` and
    // `gmeow:mapsetMusicNotation`; the former, smaller, is canonical).
    for subject in view.subjects_of_type(GM_MAPPING_SET) {
        let Some(file) = view.object_literal(&subject, GM_SSSOM_FILE) else {
            continue;
        };
        out.entry(file).or_insert_with(|| MappingSet {
            set_id: view.object_literal(&subject, GM_SET_ID).unwrap_or_default(),
            license: view
                .object_literal(&subject, GM_LICENSE)
                .unwrap_or_default(),
            comment: view
                .object_literal(&subject, GM_SET_COMMENT)
                .unwrap_or_default(),
            trailer: view
                .object_literal(&subject, GM_SET_TRAILER)
                .unwrap_or_default(),
        });
    }
}

// ── Rendering (pure — reproduces the historical bespoke TSV byte-for-byte) ────────

/// Build one checked [`SssomMapping`] row, hard-failing on a cell that would
/// corrupt the TSV (a raw tab/CR/LF). SSSOM is tab-separated, newline-delimited,
/// so such a character would silently split a value across columns or rows —
/// `purrdf::sssom::serialize_tsv` does not itself guard against this (it trusts
/// its caller), so GMEOW keeps the check here, at construction time, rather than
/// letting a corrupt cell reach the shared serializer.
#[allow(clippy::too_many_arguments)]
fn checked_mapping(
    subject_id: String,
    subject_label: Option<String>,
    predicate_id: String,
    object_id: String,
    object_label: Option<String>,
    mapping_justification: String,
    confidence: Option<f64>,
    comment: Option<String>,
) -> gmeow_errors::Result<SssomMapping> {
    check_tsv_cell("subject_id", &subject_id)?;
    if let Some(v) = &subject_label {
        check_tsv_cell("subject_label", v)?;
    }
    check_tsv_cell("predicate_id", &predicate_id)?;
    check_tsv_cell("object_id", &object_id)?;
    if let Some(v) = &object_label {
        check_tsv_cell("object_label", v)?;
    }
    check_tsv_cell("mapping_justification", &mapping_justification)?;
    if let Some(v) = &comment {
        check_tsv_cell("comment", v)?;
    }
    Ok(SssomMapping {
        subject_id,
        subject_label,
        predicate_id,
        object_id,
        object_label,
        mapping_justification,
        confidence,
        comment,
        extras: BTreeMap::new(),
    })
}

/// `None` for an empty string, `Some(value)` otherwise — the bespoke `Row`'s
/// blank-means-absent convention, lifted onto `purrdf::SssomMapping`'s
/// `Option<String>` label/comment slots.
fn opt(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

fn render_sets(
    mapping_sets: &BTreeMap<String, MappingSet>,
    by_file: &BTreeMap<String, Vec<SssomMapping>>,
    version: &str,
    release_date: &str,
) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (file, rows) in by_file {
        let meta = mapping_sets.get(file);
        out.insert(file.clone(), render_one(rows, meta, version, release_date));
    }
    out
}

/// Reject a TSV cell whose value carries a raw tab/CR/LF. SSSOM is tab-separated,
/// newline-delimited, so such a character would silently split a value across
/// columns or rows — corrupting the table. Hard-fail rather than mangle the data.
fn check_tsv_cell(column: &str, value: &str) -> gmeow_errors::Result<()> {
    if value.contains(['\t', '\r', '\n']) {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Sssom {
            detail: format!(
                "SSSOM cell `{column}` contains a tab/CR/LF that would corrupt the TSV: {value:?}"
            ),
        }));
    }
    Ok(())
}

/// The registry `prefix → namespace` curie_map a set of rows actually uses —
/// every `prefix:` token the registry recognizes across the four CURIE-bearing
/// columns (subject/predicate/object/justification), so the header declares
/// exactly the prefixes the body needs and no more.
fn used_curie_map(rows: &[SssomMapping]) -> BTreeMap<String, String> {
    let mut used: BTreeSet<&str> = BTreeSet::new();
    for r in rows {
        for tok in [
            r.subject_id.as_str(),
            r.predicate_id.as_str(),
            r.object_id.as_str(),
            r.mapping_justification.as_str(),
        ] {
            if let Some((prefix, _)) = tok.split_once(':')
                && registry_iri(prefix).is_some()
            {
                used.insert(prefix);
            }
        }
    }
    used.into_iter()
        .filter_map(|prefix| registry_iri(prefix).map(|ns| (prefix.to_owned(), ns.to_owned())))
        .collect()
}

/// The `SssomMeta` header GMEOW writes for one mapping set. `mapping_set_id`,
/// `mapping_set_version`, and `license` are an all-or-nothing trio (present only
/// when the file has a registered `gmeow:MappingSet` with a non-empty `setId`);
/// `mapping_tool`/`mapping_tool_version`/`mapping_date` are always present.
fn build_meta(
    meta: Option<&MappingSet>,
    curie_map: BTreeMap<String, String>,
    version: &str,
    release_date: &str,
) -> SssomMeta {
    let has_set_id = meta.is_some_and(|m| !m.set_id.is_empty());
    let (mapping_set_id, mapping_set_version, license) = if has_set_id {
        let m = meta.expect("has_set_id implies meta is Some");
        (
            Some(m.set_id.clone()),
            Some(version.to_owned()),
            Some(m.license.clone()),
        )
    } else {
        (None, None, None)
    };
    let comment = meta
        .filter(|m| !m.comment.is_empty())
        .map(|m| json_quote_ascii(&collapse_whitespace(&m.comment)));
    SssomMeta {
        mapping_set_id,
        mapping_set_version,
        license,
        mapping_tool: Some(
            "gmeow-dev sync --mode update --outputs generated (mappings)".to_owned(),
        ),
        mapping_tool_version: Some(version.to_owned()),
        mapping_date: Some(release_date.to_owned()),
        comment,
        curie_map,
        extra: BTreeMap::new(),
    }
}

/// Splice GMEOW's refused/deferred-mapping provenance trailer back into the
/// canonical `purrdf::sssom::serialize_tsv` output, right before the TSV
/// column-header row. `purrdf`'s SSSOM codec deliberately treats a `# #…` line
/// as a documentation comment, not mapping-set metadata (its `parse_tsv` skips
/// such lines outright — see its module doc), so it has no `SssomMeta` slot for
/// this content; splicing it in at render time is GMEOW's own provenance
/// convention layered on top of the canonical serializer, not a competing SSSOM
/// serializer.
fn splice_trailer(tsv: String, trailer: &str) -> String {
    if trailer.is_empty() {
        return tsv;
    }
    let lines: Vec<&str> = tsv.lines().collect();
    let insert_at = lines
        .iter()
        .position(|line| !line.starts_with('#'))
        .unwrap_or(lines.len());
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + trailer.lines().count());
    for (i, line) in lines.iter().enumerate() {
        if i == insert_at {
            for trailer_line in trailer.lines() {
                out.push(format!(
                    "# #{}",
                    trailer_line.strip_prefix('#').unwrap_or(trailer_line)
                ));
            }
        }
        out.push((*line).to_owned());
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn render_one(
    rows: &[SssomMapping],
    meta: Option<&MappingSet>,
    version: &str,
    release_date: &str,
) -> String {
    let curie_map = used_curie_map(rows);
    let set = SssomMappingSet {
        meta: build_meta(meta, curie_map, version, release_date),
        mappings: rows.to_vec(),
    };
    let tsv = purrdf::sssom::serialize_tsv(&set);
    match meta {
        Some(m) if !m.trailer.is_empty() => splice_trailer(tsv, &m.trailer),
        _ => tsv,
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn json_quote_ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if c.is_ascii() => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

    #[test]
    fn render_one_emits_canonical_tsv() {
        let table = ns_to_prefix();
        let make = |subj: &str, pred: &str, obj: &str, c: Option<f64>| {
            checked_mapping(
                sssom_id(subj, table),
                None,
                sssom_id(pred, table),
                sssom_id(obj, table),
                None,
                sssom_id(DEFAULT_JUSTIFICATION, table),
                c,
                None,
            )
            .expect("well-formed row")
        };
        // Two rows, deliberately out of (subject, predicate, object) order.
        let rows = vec![
            make(
                &format!("{GMEOW}Zeta"),
                "http://www.w3.org/2004/02/skos/core#closeMatch",
                &format!("{GMEOW}Bar"),
                Some(0.8),
            ),
            make(
                &format!("{GMEOW}Alpha"),
                "http://www.w3.org/2004/02/skos/core#exactMatch",
                &format!("{GMEOW}Foo"),
                Some(1.0),
            ),
        ];
        let meta = MappingSet {
            set_id: "https://blackcatinformatics.ca/gmeow/mappings/demo".to_owned(),
            license: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
            comment: "Demo  set\nwith   wrap".to_owned(),
            trailer: "# REFUSED nothing here".to_owned(),
        };
        let text = render_one(&rows, Some(&meta), "0.1.0", "2026-06-03");
        let expected = "\
# mapping_set_id: https://blackcatinformatics.ca/gmeow/mappings/demo
# mapping_set_version: 0.1.0
# license: https://creativecommons.org/licenses/by/4.0/
# mapping_tool: gmeow-dev sync --mode update --outputs generated (mappings)
# mapping_tool_version: 0.1.0
# mapping_date: 2026-06-03
# comment: \"Demo set with wrap\"
# curie_map:
#   gmeow: https://blackcatinformatics.ca/gmeow/
#   semapv: https://w3id.org/semapv/vocab/
#   skos: http://www.w3.org/2004/02/skos/core#
# # REFUSED nothing here
subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment
gmeow:Alpha\tskos:exactMatch\tgmeow:Foo\tsemapv:ManualMappingCuration\t1.0\t
gmeow:Zeta\tskos:closeMatch\tgmeow:Bar\tsemapv:ManualMappingCuration\t0.8\t
";
        assert_eq!(text, expected);
    }

    #[test]
    fn label_column_appears_only_when_populated() {
        let table = ns_to_prefix();
        let row = checked_mapping(
            sssom_id(&format!("{GMEOW}Foo"), table),
            Some("Foo label".to_owned()),
            sssom_id("http://www.w3.org/2004/02/skos/core#exactMatch", table),
            sssom_id(&format!("{GMEOW}Bar"), table),
            None,
            sssom_id(DEFAULT_JUSTIFICATION, table),
            None,
            None,
        )
        .expect("well-formed row");
        let text = render_one(&[row], None, "0.1.0", "2026-06-03");
        let header_row = text
            .lines()
            .find(|l| l.starts_with("subject_id"))
            .expect("column header");
        assert_eq!(
            header_row,
            "subject_id\tsubject_label\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment"
        );
        assert!(!text.contains("mapping_set_id"));
    }

    #[test]
    fn checked_mapping_rejects_tab_in_cell() {
        let table = ns_to_prefix();
        let err = checked_mapping(
            sssom_id(&format!("{GMEOW}Foo"), table),
            Some("has\ttab".to_owned()),
            sssom_id("http://www.w3.org/2004/02/skos/core#exactMatch", table),
            sssom_id(&format!("{GMEOW}Bar"), table),
            None,
            sssom_id(DEFAULT_JUSTIFICATION, table),
            None,
            None,
        )
        .expect_err("a cell with a tab must be rejected");
        assert!(err.message().contains("subject_label"), "{err}");
    }

    #[test]
    fn lower_sssom_extracts_over_dslview() {
        use purrdf::{RdfDatasetBuilder, RdfLiteral};

        let mut b = RdfDatasetBuilder::new();
        // Intern every term to a local first to avoid nested `&mut b` borrows, then
        // push the `(s, p, o)` triples.
        let iri = |s: &str| s.to_owned();
        let triple = |b: &mut RdfDatasetBuilder,
                      s: &str,
                      p: &str,
                      o_iri: Option<&str>,
                      o_lit: Option<&str>| {
            let s = b.intern_iri(&iri(s));
            let p = b.intern_iri(&iri(p));
            let o = match (o_iri, o_lit) {
                (Some(o), _) => b.intern_iri(&iri(o)),
                (_, Some(l)) => b.intern_literal(RdfLiteral::simple(l.to_owned())),
                _ => unreachable!(),
            };
            b.push_quad(s, p, o, None);
        };
        let eq1 = format!("{GMEOW}eq1");
        let skos_exact = "http://www.w3.org/2004/02/skos/core#exactMatch";
        triple(&mut b, &eq1, RDF_TYPE, Some(GM_TERM_EQUIVALENCE), None);
        triple(
            &mut b,
            &eq1,
            GM_ALIGN_SUBJECT,
            Some(&format!("{GMEOW}Foo")),
            None,
        );
        triple(&mut b, &eq1, GM_ALIGN_PREDICATE, Some(skos_exact), None);
        triple(
            &mut b,
            &eq1,
            GM_ALIGN_OBJECT,
            Some(&format!("{GMEOW}Bar")),
            None,
        );
        triple(&mut b, &eq1, GM_SSSOM_FILE, None, Some("demo.sssom.tsv"));
        triple(&mut b, &eq1, GM_CONFIDENCE, None, Some("1.0"));
        let set1 = format!("{GMEOW}set1");
        triple(&mut b, &set1, RDF_TYPE, Some(GM_MAPPING_SET), None);
        triple(&mut b, &set1, GM_SSSOM_FILE, None, Some("demo.sssom.tsv"));
        triple(
            &mut b,
            &set1,
            GM_SET_ID,
            None,
            Some(&format!("{GMEOW}mappings/demo")),
        );
        triple(
            &mut b,
            &set1,
            GM_LICENSE,
            None,
            Some("https://creativecommons.org/licenses/by/4.0/"),
        );
        let ds = b.freeze().expect("freeze");
        let view = DslView::new(&ds);

        // Build the materialized correspondence lookup from the same view, exactly as the
        // pipeline stage does, so the ledger gate consumes the materialized typed relation.
        let empty = purrdf::parse_dataset(b"", "application/n-triples", None).expect("empty");
        let (_program, lookup) =
            crate::projections::correspondence_frontend::transpile_correspondences_indexed(
                &view,
                &DslView::new(&empty),
            )
            .expect("transpile lookup");
        let out = lower_sssom(&view, "0.1.0", "2026-06-03", &lookup).expect("lower sssom");
        let tsv = out.sets.get("demo.sssom.tsv").expect("one set emitted");
        assert!(
            tsv.contains("# mapping_set_id: https://blackcatinformatics.ca/gmeow/mappings/demo")
        );
        assert!(tsv.ends_with(
            "gmeow:Foo\tskos:exactMatch\tgmeow:Bar\tsemapv:ManualMappingCuration\t1.0\t\n"
        ));
        assert_eq!(
            alignment_terms(&view),
            BTreeSet::from([format!("{GMEOW}Foo"), format!("{GMEOW}Bar")])
        );
    }

    #[test]
    fn lower_sssom_emits_projection_binding_rows() {
        let ttl = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix schema: <https://schema.org/> .
@prefix odrl:  <http://www.w3.org/ns/odrl/2/> .

gmeow:set1 a gmeow:MappingSet ;
    gmeow:sssomFile "demo.sssom.tsv" ;
    gmeow:setId "https://blackcatinformatics.ca/gmeow/mappings/demo" ;
    gmeow:license "https://creativecommons.org/licenses/by/4.0/" .

gmeow:mapName a gmeow:ProjectionMapping ;
    gmeow:subjectLabel "GMEOW name" ;
    gmeow:objectLabel "schema.org name" ;
    gmeow:justification <https://w3id.org/semapv/vocab/ManualMappingCuration> ;
    gmeow:comment "Curated exact property correspondence." ;
    gmeow:hasMappingPattern [
        gmeow:anchor "s" ; gmeow:value "name" ;
        gmeow:atom ( [ gmeow:subjectVar "s" ; gmeow:predicate gmeow:name ; gmeow:objectVar "name" ] ) ;
        gmeow:edoalSource gmeow:name
    ] ;
    gmeow:hasBinding [
        gmeow:profile "schema-org" ; gmeow:toPredicate schema:name ;
        gmeow:relation "=" ; gmeow:confidence 0.9 ;
        gmeow:emitSssom true ; gmeow:sssomPredicate skos:exactMatch ;
        gmeow:sssomFile "demo.sssom.tsv"
    ] .

gmeow:mapActionReproduce a gmeow:ProjectionMapping ;
    gmeow:hasMappingPattern [
        gmeow:anchor "rule" ;
        gmeow:atom ( [ gmeow:subjectVar "rule" ; gmeow:predicate gmeow:ruleAction ; gmeow:objectValue gmeow:actionReproduce ] )
    ] ;
    gmeow:hasBinding [
        gmeow:profile "odrl" ; gmeow:relation "=" ; gmeow:confidence 0.85 ;
        gmeow:emitSssom true ; gmeow:sssomPredicate skos:exactMatch ;
        gmeow:sssomFile "demo.sssom.tsv" ;
        gmeow:templateAtoms ( [ gmeow:tSubj "rule" ; gmeow:tPred odrl:action ; gmeow:tObjValue odrl:reproduce ] )
    ] .
"#;
        let ds = purrdf::parse_dataset(ttl, "text/turtle", None).expect("parse projection ttl");
        let view = DslView::new(&ds);
        let empty = purrdf::parse_dataset(b"", "application/n-triples", None).expect("empty");
        let (_program, lookup) =
            crate::projections::correspondence_frontend::transpile_correspondences_indexed(
                &view,
                &DslView::new(&empty),
            )
            .expect("transpile lookup");

        let out = lower_sssom(&view, "0.1.0", "2026-06-03", &lookup).expect("lower sssom");
        let tsv = out.sets.get("demo.sssom.tsv").expect("one set emitted");
        assert!(tsv.contains(
            "subject_id\tsubject_label\tpredicate_id\tobject_id\tobject_label\tmapping_justification\tconfidence\tcomment"
        ));
        assert!(tsv.contains(
            "gmeow:name\tGMEOW name\tskos:exactMatch\tschema:name\tschema.org name\tsemapv:ManualMappingCuration\t0.9\tCurated exact property correspondence."
        ));
        assert!(tsv.contains(
            "gmeow:actionReproduce\t\tskos:exactMatch\todrl:reproduce\t\tsemapv:ManualMappingCuration\t0.85\t"
        ));
        assert_eq!(out.ledger.len(), 2);
        assert_eq!(
            alignment_terms(&view),
            BTreeSet::from([
                format!("{GMEOW}actionReproduce"),
                format!("{GMEOW}name"),
                "http://www.w3.org/ns/odrl/2/reproduce".to_owned(),
                "https://schema.org/name".to_owned(),
            ])
        );
    }

    /// Transpile + lower a native-form corpus, returning the typed program and the SSSOM
    /// sets so a test can assert BOTH artifacts materialize from one shared derivation.
    fn transpile_and_lower(
        ttl: &[u8],
    ) -> gmeow_errors::Result<(
        crate::projections::correspondence::CorrespondenceProgram,
        SssomLowering,
    )> {
        let ds = purrdf::parse_dataset(ttl, "text/turtle", None).expect("parse native ttl");
        let view = DslView::new(&ds);
        let empty = purrdf::parse_dataset(b"", "application/n-triples", None).expect("empty");
        let (program, lookup) =
            crate::projections::correspondence_frontend::transpile_correspondences_indexed(
                &view,
                &DslView::new(&empty),
            )?;
        let lowering = lower_sssom(&view, "0.1.0", "2026-06-03", &lookup)?;
        Ok((program, lowering))
    }

    /// The CANONICAL native alignment-cell form (issue #1200 R4/AC3). Each cell is one
    /// RDF-1.2 asserting-annotation `s skos:*Match o {| … |}`; the reifier's annotation
    /// block carries the SSSOM/correspondence fields. `gmeow:sssomFile` is the REQUIRED
    /// discriminator. The migration tool must emit byte-compatible output of this shape.
    const NATIVE_PROLOGUE: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix schema: <https://schema.org/> .
@prefix gufo:  <http://purl.org/nemo/gufo#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .
";

    #[test]
    fn native_owl_and_rdfs_alignment_predicates_are_read() {
        // The authored gmeow:alignPredicate surface carried OWL/RDFS alignment predicates
        // (owl:equivalentClass/equivalentProperty/sameAs, rdfs:subClassOf/subPropertyOf), not
        // only the five skos:*Match names — 88 such cells exist in the corpus. The native
        // reader MUST read them too, else the greenfield align* deletion would orphan them.
        use crate::ir::{CorrespondenceRelation, MorphismClass};
        let cases: &[(&str, &str, CorrespondenceRelation, MorphismClass)] = &[
            (
                "owl",
                "equivalentClass",
                CorrespondenceRelation::Equiv,
                MorphismClass::WellBehavedLens,
            ),
            (
                "owl",
                "equivalentProperty",
                CorrespondenceRelation::Equiv,
                MorphismClass::WellBehavedLens,
            ),
            (
                "rdfs",
                "subClassOf",
                CorrespondenceRelation::Subsumes,
                MorphismClass::LossyLens,
            ),
            (
                "rdfs",
                "subPropertyOf",
                CorrespondenceRelation::Subsumes,
                MorphismClass::LossyLens,
            ),
        ];
        for (pfx, local, relation, mclass) in cases {
            let ttl = format!(
                "{NATIVE_PROLOGUE}@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:OnlineAccount {pfx}:{local} schema:Thing {{|
    gmeow:sssomFile      \"gmeow-accounts.sssom.tsv\" ;
    gmeow:justification  semapv:ManualMappingCuration ;
    gmeow:confidence     0.9
|}} .
"
            );
            let (program, _lowering) =
                transpile_and_lower(ttl.as_bytes()).expect("native owl/rdfs cell lowers");
            assert_eq!(program.correspondences.len(), 1, "{pfx}:{local}");
            let corr = &program.correspondences[0];
            assert_eq!(corr.relation, *relation, "{pfx}:{local} relation");
            assert_eq!(corr.morphism_class, *mclass, "{pfx}:{local} class");
        }
    }

    #[test]
    fn native_five_match_predicates_materialize_row_and_correspondence() {
        use crate::ir::{CorrespondenceRelation, MorphismClass};
        // predicate local-name → (expected relation, expected morphism class from the band).
        let cases: &[(&str, CorrespondenceRelation, MorphismClass)] = &[
            (
                "exactMatch",
                CorrespondenceRelation::Equiv,
                MorphismClass::WellBehavedLens,
            ),
            (
                "closeMatch",
                CorrespondenceRelation::Overlaps,
                MorphismClass::AffineCorrespondence,
            ),
            (
                "broadMatch",
                CorrespondenceRelation::Subsumes,
                MorphismClass::LossyLens,
            ),
            (
                "narrowMatch",
                CorrespondenceRelation::SubsumedBy,
                MorphismClass::LossyLens,
            ),
            (
                "relatedMatch",
                CorrespondenceRelation::RelatedMatch,
                MorphismClass::AffineCorrespondence,
            ),
        ];
        for (local, relation, mclass) in cases {
            let ttl = format!(
                "{NATIVE_PROLOGUE}
gmeow:VirtualLocation skos:{local} schema:VirtualLocation {{|
    gmeow:sssomFile      \"gmeow-places.sssom.tsv\" ;
    gmeow:justification  semapv:ManualMappingCuration ;
    gmeow:confidence     0.9
|}} .
"
            );
            let (program, lowering) =
                transpile_and_lower(ttl.as_bytes()).expect("native cell lowers");

            // One typed correspondence, carrying the band-derived relation + class.
            assert_eq!(program.correspondences.len(), 1, "{local}");
            let corr = &program.correspondences[0];
            assert_eq!(corr.relation, *relation, "{local} relation");
            assert_eq!(corr.morphism_class, *mclass, "{local} class");

            // One SSSOM row into the discriminator's file.
            let tsv = lowering
                .sets
                .get("gmeow-places.sssom.tsv")
                .unwrap_or_else(|| panic!("{local}: set emitted"));
            assert!(
                tsv.contains(&format!(
                    "gmeow:VirtualLocation\tskos:{local}\tschema:VirtualLocation\tsemapv:ManualMappingCuration\t0.9\t"
                )),
                "{local} row:\n{tsv}"
            );
        }
    }

    #[test]
    fn native_grounding_cell_preserves_all_fields_and_passes_invariants() {
        let ttl = format!(
            "{NATIVE_PROLOGUE}
logic:Individual skos:closeMatch gufo:Individual {{|
    a                       logic:GroundingCorrespondence ;
    gmeow:sssomFile         \"gmeow-logic.sssom.tsv\" ;
    gmeow:justification     semapv:ManualMappingCuration ;
    logic:sourceEndpoint    logic:Individual ;
    logic:targetEndpoint    gufo:Individual ;
    logic:morphismClass     logic:AffineCorrespondence ;
    logic:morphismKind      logic:InstitutionMorphism ;
    logic:preservationKind  logic:SoundUnderApproximation
|}} .
"
        );
        let (program, lowering) =
            transpile_and_lower(ttl.as_bytes()).expect("grounding native cell lowers");

        // The cell reads back as a grounding correspondence with every field preserved.
        let cells = equivalence_cells(&DslView::new(
            &purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse"),
        ));
        assert_eq!(cells.len(), 1);
        let cell = &cells[0];
        assert!(cell.grounding);
        assert_eq!(cell.sssom_file, "gmeow-logic.sssom.tsv");
        assert_eq!(
            cell.justification.as_deref(),
            Some("https://w3id.org/semapv/vocab/ManualMappingCuration")
        );
        assert_eq!(
            cell.source_endpoint.as_deref(),
            Some("https://blackcatinformatics.ca/logic/Individual")
        );
        assert_eq!(
            cell.target_endpoint.as_deref(),
            Some("http://purl.org/nemo/gufo#Individual")
        );
        assert_eq!(
            cell.morphism_class.as_deref(),
            Some("https://blackcatinformatics.ca/logic/AffineCorrespondence")
        );
        assert_eq!(
            cell.preservation.as_deref(),
            Some("https://blackcatinformatics.ca/logic/SoundUnderApproximation")
        );

        // The grounding correspondence and its SSSOM row both materialize.
        assert_eq!(program.correspondences.len(), 1);
        assert!(program.correspondences[0].grounding);
        assert!(
            lowering
                .sets
                .get("gmeow-logic.sssom.tsv")
                .expect("set emitted")
                .contains("logic:Individual\tskos:closeMatch\tgufo:Individual")
        );
    }

    #[test]
    fn native_grounding_cell_missing_preservation_hard_fails() {
        // Same grounding cell as above but with logic:preservationKind DROPPED — the
        // grounding invariant must hard-fail with the same diagnostic the legacy reader did.
        let ttl = format!(
            "{NATIVE_PROLOGUE}
logic:Individual skos:closeMatch gufo:Individual {{|
    a                       logic:GroundingCorrespondence ;
    gmeow:sssomFile         \"gmeow-logic.sssom.tsv\" ;
    gmeow:justification     semapv:ManualMappingCuration ;
    logic:sourceEndpoint    logic:Individual ;
    logic:targetEndpoint    gufo:Individual ;
    logic:morphismClass     logic:AffineCorrespondence ;
    logic:morphismKind      logic:InstitutionMorphism
|}} .
"
        );
        let err = match transpile_and_lower(ttl.as_bytes()) {
            Ok(_) => panic!("missing preservation must fail"),
            Err(err) => err,
        };
        assert!(
            err.message().contains("preservationKind"),
            "diagnostic should name the missing field: {err}"
        );
    }

    #[test]
    fn bare_skos_exactmatch_without_sssomfile_is_ignored() {
        // A-Box coreference: a bare (un-annotated) skos:exactMatch with no reifier and no
        // gmeow:sssomFile discriminator MUST NOT be swept into the alignment corpus.
        let ttl = format!(
            "{NATIVE_PROLOGUE}
gmeow:Thing skos:exactMatch schema:Thing .

# A reified skos:*Match WITHOUT gmeow:sssomFile is also NOT an alignment cell.
gmeow:Other skos:exactMatch schema:Other {{|
    gmeow:confidence 0.5
|}} .
"
        );
        let (program, lowering) =
            transpile_and_lower(ttl.as_bytes()).expect("no cells still lowers cleanly");
        assert!(
            program.correspondences.is_empty(),
            "no alignment cell should be extracted"
        );
        assert!(lowering.sets.is_empty(), "no SSSOM set should be emitted");
        assert!(
            equivalence_cells(&DslView::new(
                &purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse")
            ))
            .is_empty()
        );
    }

    #[test]
    fn projection_binding_exactmatch_overclaim_is_rejected() {
        let ttl = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix schema: <https://schema.org/> .

gmeow:mapLossyName a gmeow:ProjectionMapping ;
    gmeow:hasMappingPattern [
        gmeow:anchor "s" ; gmeow:value "name" ;
        gmeow:atom ( [ gmeow:subjectVar "s" ; gmeow:predicate gmeow:name ; gmeow:objectVar "name" ] ) ;
        gmeow:edoalSource gmeow:name
    ] ;
    gmeow:hasBinding [
        gmeow:profile "schema-org" ; gmeow:toPredicate schema:name ;
        gmeow:relation "<=" ; gmeow:confidence 0.9 ;
        gmeow:emitSssom true ; gmeow:sssomPredicate skos:exactMatch ;
        gmeow:sssomFile "demo.sssom.tsv"
    ] .
"#;
        let ds = purrdf::parse_dataset(ttl, "text/turtle", None).expect("parse projection ttl");
        let view = DslView::new(&ds);
        let empty = purrdf::parse_dataset(b"", "application/n-triples", None).expect("empty");
        let (_program, lookup) =
            crate::projections::correspondence_frontend::transpile_correspondences_indexed(
                &view,
                &DslView::new(&empty),
            )
            .expect("transpile lookup");
        let err = match lower_sssom(&view, "0.1.0", "2026-06-03", &lookup) {
            Ok(_) => panic!("overclaim should be rejected"),
            Err(err) => err,
        };
        assert!(err.message().contains("Overclaim"), "{err}");
        assert!(err.message().contains("exactMatch"), "{err}");
    }
}
