// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SSSOM correspondence lowering: native RDF-1.2 alignment cells → SSSOM TSV.
//!
//! SSSOM is the 1:1-lattice-band lowering of the correspondence calculus. Each
//! native alignment cell compiles to exactly one SSSOM row; the target drops the
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
use crate::projections::correspondence_frontend::CorrespondenceAnalysis;
use crate::projections::correspondence_gate::assert_relation_no_overclaim;
use crate::projections::get_leg::{MappingPattern, ProfileBinding, ProjectionCell};
use crate::projections::{ProjectionResult, correspondence_result};

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

/// One native alignment cell — compiles to exactly one SSSOM row. IRIs are
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
    pub confidence: Option<crate::ir::UnitInterval>,
    pub justification: Option<String>,
    /// Optional authored law-spine rung. Absent cells retain the predicate-derived SSSOM
    /// default; grounding bridges author this explicitly so a commitment shift can never
    /// be flattened into an ordinary lens.
    pub morphism_class: Option<String>,
    /// Optional authored satisfaction/commitment qualifier.
    pub morphism_kind: Option<String>,
    /// Optional authored per-correspondence preservation judgment.
    pub preservation: Option<String>,
    /// Explicit source endpoint of a grounding correspondence. Ordinary SSSOM cells may
    /// omit it; grounding cells must carry it and it must agree with the match subject.
    pub source_endpoint: Option<String>,
    /// Explicit target endpoint of a grounding correspondence. Ordinary SSSOM cells may
    /// omit it; grounding cells must carry it and it must agree with the match object.
    pub target_endpoint: Option<String>,
    /// Whether the frontend cell is explicitly a `logic:GroundingCorrespondence`.
    pub grounding: bool,
    comment: String,
    /// Structured per-correspondence drop notes (`gmeow:lossyDrop`) — the specific
    /// constructs this by-reference lowering does not carry (e.g. a loop unrolls, a
    /// concurrent composition serializes, a per-outcome compensation is omitted). Folded
    /// into the report's per-correspondence residue, distinct from the human `comment`.
    pub(crate) lossy_drops: Vec<purrdf::RdfLiteral>,
    pub sssom_file: String,
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
/// Keeping this beside the parsed get-leg model lets a correspondence carry an alignment
/// cell's evidence, labels, or explanatory note without discarding them.
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
struct SssomSources<'a> {
    equivalences: &'a [EquivalenceCell],
    projections: &'a [ProjectionCell],
    projection_metadata: BTreeMap<String, ProjectionSssomMetadata>,
    mapping_sets: BTreeMap<String, MappingSet>,
}

/// The artifacts + per-correspondence loss ledger of the SSSOM lowering.
pub struct SssomLowering {
    /// Bare file name (e.g. `gmeow-accessibility.sssom.tsv`) → TSV.
    pub sets: BTreeMap<String, String>,
    /// One [`ProjectionResult`] per native alignment cell correspondence — SSSOM
    /// always drops the caveat/law/leg structure and world/standpoint scope, so every
    /// cell contributes a preservation row.
    pub ledger: Vec<ProjectionResult>,
    /// The per-correspondence loss store this lowering interned every drop into (keyed by
    /// target focus). The mappings stage unions it into the single report loss store so the
    /// SSSOM rows' `gmeow:lossyDrop` records read back from the SAME substrate ledger.
    pub loss: crate::loss_ledger::LossLedger,
}

/// Lower every native alignment cell in `view` to its SSSOM TSV, keyed by bare file
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
    lookup: &CorrespondenceAnalysis,
) -> gmeow_errors::Result<SssomLowering> {
    let mut loss = crate::loss_ledger::LossLedger::new();
    let sources = collect_sources(view, lookup);
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

/// Build one preservation row per native alignment cell correspondence, running the
/// overclaim gate over each emitted predicate. The typed `(relation, morphism class,
/// morphism kind)` is CONSUMED from the materialized correspondence set (`lookup`) — the
/// single source of truth — not re-derived inline here.
fn build_rows_and_ledger(
    sources: &SssomSources<'_>,
    lookup: &CorrespondenceAnalysis,
    loss: &mut crate::loss_ledger::LossLedger,
) -> gmeow_errors::Result<(RowsByFile, PreservationLedger)> {
    let table = ns_to_prefix();
    let mut by_file: RowsByFile = BTreeMap::new();
    let mut ledger: PreservationLedger = Vec::new();
    for cell in sources.equivalences {
        // Consume the typed relation/class/kind from the materialized correspondence keyed
        // by the complete authored alignment declaration. A miss is a HARD
        // FAIL — every authored cell is transpiled (no-optionality).
        let typed = lookup.equivalence(cell)?;
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
        let mut residue = Vec::new();
        // Author-declared per-correspondence drops (gmeow:lossyDrop) — the specific
        // constructs a by-reference engine surface cannot carry (a loop unrolls/errors, a
        // concurrent composition serializes, a per-outcome compensation is omitted) — are
        // structured residue notes, so the loss ledger records WHAT each lowering drops
        // rather than leaving it to prose.
        if cell.confidence.is_some() {
            residue.push("get-leg: confidence is projected as binary64; original RDF datatype and lexical identity are not carried by SSSOM".to_owned());
        }
        residue.extend(
            cell.lossy_drops
                .iter()
                .map(|literal| literal.lexical_form.clone()),
        );
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
                cell.confidence
                    .as_ref()
                    .map(crate::ir::UnitInterval::projection_f64),
                opt(cell.comment.clone()),
            )?);
    }

    for cell in sources.projections {
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

            let typed = lookup.binding(cell, binding)?;
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
                        binding
                            .confidence
                            .as_ref()
                            .map(crate::ir::UnitInterval::projection_f64),
                        opt(metadata.comment.clone()),
                    )?);
            }

            let mut residue = Vec::new();
            if binding.confidence.is_some() {
                residue.push("get-leg: confidence is projected as binary64; original RDF datatype and lexical identity are not carried by SSSOM".to_owned());
            }
            residue.extend(
                binding
                    .lossy_drops
                    .iter()
                    .map(|d| format!("get-leg profile loss: {d}")),
            );
            let key = crate::projections::get_leg::binding_key(cell, binding);
            // Profile-binding cells are gmeow:ProjectionMapping views (no clean
            // subject/object IRI pair); their residue stays whole-program.
            ledger.push(correspondence_result(loss, "sssom", &key, residue, None));
        }
    }
    Ok((by_file, ledger))
}

/// Every IRI participating in an SSSOM equivalence (both subject and object position)
/// — the alignment-terms set the projection lints consume.
pub fn alignment_terms(
    analysis: &CorrespondenceAnalysis,
) -> gmeow_errors::Result<BTreeSet<String>> {
    let mut terms = BTreeSet::new();
    for cell in analysis.alignment_cells() {
        terms.insert(cell.subject.clone());
        terms.insert(cell.obj.clone());
    }
    for cell in analysis.projection_cells() {
        for binding in &cell.bindings {
            if binding.emit_sssom {
                for (subject, object) in projection_sssom_pairs(cell, binding)? {
                    terms.insert(subject);
                    terms.insert(object);
                }
            }
        }
    }
    Ok(terms)
}

// ── Extraction (over the oxigraph-free DslView) ──────────────────────────────────

/// Every native alignment cell discovered over `view`, in extraction order — the
/// frontend transpiler's input. Shares [`extract_equivalences`] with the SSSOM renderer,
/// so the typed correspondence set and the rendered TSV read the store identically.
///
/// # Errors
///
/// Hard-fails if a reified statement CARRIES the `gmeow:sssomFile` discriminator (i.e. is
/// an alignment cell) but is malformed — a non-IRI match object, or a predicate the
/// alignment lattice does not classify. Such a cell would otherwise silently vanish from
/// the correspondence corpus (no-optionality forbids the silent drop).
pub fn equivalence_cells(view: &DslView) -> gmeow_errors::Result<Vec<EquivalenceCell>> {
    let mut out = Vec::new();
    extract_equivalences(view, &mut out)?;
    Ok(out)
}

fn collect_sources<'a>(view: &DslView, analysis: &'a CorrespondenceAnalysis) -> SssomSources<'a> {
    let equivalences = analysis.alignment_cells();
    let mut mapping_sets = BTreeMap::new();
    extract_mapping_sets(view, &mut mapping_sets);
    let projections = analysis.projection_cells();
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
    SssomSources {
        equivalences,
        projections,
        projection_metadata,
        mapping_sets,
    }
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
/// names. Alignment cells also carry OWL/RDFS alignment predicates
/// (`owl:equivalentClass`/`equivalentProperty`, `rdfs:subClassOf`/`subPropertyOf`,
/// `owl:sameAs`), so the native reader accepts them too. Note: an
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
/// `skos:*Match` (or `owl:`/`rdfs:` alignment) triple whose reifier carries the
/// `gmeow:sssomFile` discriminator. This is the SOLE alignment-cell reader.
///
/// The discriminator is load-bearing: a bare `ex:x skos:exactMatch ex:y` with no reifier
/// (instance coreference — `examples/authority-links.ttl`, `music/fixtures/*`) has no
/// reified statement here at all, and a reified `skos:*Match` without `gmeow:sssomFile` is
/// skipped, so A-Box coreference is never swept into the alignment corpus.
///
/// # Errors
///
/// A reifier that DOES carry `gmeow:sssomFile` (so it IS an alignment cell) but whose
/// match triple is malformed — a non-IRI object, or a predicate the alignment lattice
/// does not classify — is a HARD FAIL. This is the well-formedness gate the mapping-DSL
/// SHACL alignment-cell shape used to enforce, moved into this fail-closed reader
/// (the reifier IS the cell; a malformed cell must never be silently dropped).
fn extract_native_equivalences(
    view: &DslView,
    out: &mut Vec<EquivalenceCell>,
) -> gmeow_errors::Result<()> {
    for stmt in view.reified_statements() {
        // A present but malformed discriminator is an invalid cell, not absence.
        let Some(sssom_file) = stmt.annotation_literal(GM_SSSOM_FILE)? else {
            continue;
        };
        let (
            purrdf::TermRef::Iri(subject),
            purrdf::TermRef::Iri(predicate),
            purrdf::TermRef::Iri(object),
        ) = stmt.triple()?
        else {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Sssom {
                detail: format!(
                    "alignment cell {:?} carries gmeow:sssomFile but its match subject, predicate or object is not an IRI",
                    stmt.reifier()
                ),
            }));
        };
        if !is_alignment_predicate(predicate) {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Sssom {
                detail: format!(
                    "alignment cell on <{subject}> carries gmeow:sssomFile but its match predicate <{predicate}> is not an alignment predicate"
                ),
            }));
        }
        let confidence = stmt
            .scalar_annotation(GM_CONFIDENCE)?
            .map(|term| {
                crate::ir::UnitInterval::from_term(view.dataset(), term).map_err(|error| {
                    gmeow_errors::Diag::of_kind(crate::error::Sssom {
                        detail: format!(
                            "alignment reifier {:?}, gmeow:confidence: {error}",
                            stmt.reifier()
                        ),
                    })
                })
            })
            .transpose()?;
        out.push(EquivalenceCell {
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            obj: object.to_owned(),
            confidence,
            justification: stmt.annotation_iri(GM_JUSTIFICATION)?.map(str::to_owned),
            morphism_class: stmt
                .annotation_iri(LOGIC_MORPHISM_CLASS)?
                .map(str::to_owned),
            morphism_kind: stmt.annotation_iri(LOGIC_MORPHISM_KIND)?.map(str::to_owned),
            preservation: stmt
                .annotation_iri(LOGIC_PRESERVATION_KIND)?
                .map(str::to_owned),
            source_endpoint: stmt
                .annotation_iri(LOGIC_SOURCE_ENDPOINT)?
                .map(str::to_owned),
            target_endpoint: stmt
                .annotation_iri(LOGIC_TARGET_ENDPOINT)?
                .map(str::to_owned),
            grounding: stmt.annotation_has_type(LOGIC_GROUNDING_CORRESPONDENCE),
            comment: stmt
                .annotation_literal(GM_COMMENT)?
                .unwrap_or_default()
                .to_owned(),
            lossy_drops: stmt.annotation_rdf_literals(GM_LOSSY_DROP)?,
            sssom_file: sssom_file.to_owned(),
            subject_label: stmt
                .annotation_literal(GM_SUBJECT_LABEL)?
                .unwrap_or_default()
                .to_owned(),
            object_label: stmt
                .annotation_literal(GM_OBJECT_LABEL)?
                .unwrap_or_default()
                .to_owned(),
        });
    }
    // Sort only selected typed cells, borrowing keys instead of repeatedly cloning
    // all reified statements and their object sort keys during comparison.
    out.sort_by(|a, b| (&a.subject, &a.predicate, &a.obj).cmp(&(&b.subject, &b.predicate, &b.obj)));

    Ok(())
}

/// The native RDF-1.2 reader is the SOLE alignment-cell reader — the legacy
/// reified-type alignment cell path was removed (greenfield, no separately-authored
/// second path).
fn extract_equivalences(
    view: &DslView,
    out: &mut Vec<EquivalenceCell>,
) -> gmeow_errors::Result<()> {
    extract_native_equivalences(view, out)
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
///
/// **Retirement condition.** This exists only because the SSSOM codec has no
/// set-level provenance slot that survives a `parse_tsv` round trip. When one
/// lands upstream, the trailer rides in `SssomMeta`, the splice becomes dead
/// weight, and this function is deleted rather than kept as a second way to
/// spell the same content — the doctrine is that purrdf owns the output formats.
/// The condition is stated rather than a tracker id because the issue-refs
/// policy forbids tracker provenance in authored prose, and because a condition
/// stays true after the tracker item is renumbered, closed, or superseded.
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
    let set = SssomMappingSet::new(
        build_meta(meta, curie_map, version, release_date),
        rows.to_vec(),
    );
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

#[path = "sssom.tests.rs"]
#[cfg(test)]
mod tests;
