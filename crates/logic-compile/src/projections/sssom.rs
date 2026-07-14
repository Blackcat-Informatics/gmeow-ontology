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
//! The renderer reproduces GMEOW's bespoke SSSOM TSV byte-for-byte: the YAML-ish `#`
//! header with `curie_map`, the dynamic column set (a `*_label` column appears only
//! when some row populates it), refused-mapping trailers folded in as `# #`
//! comments, and rows sorted by `(subject_id, predicate_id, object_id)`. Extraction
//! runs over the oxigraph-free [`DslView`]; the version/date come from the caller
//! (which reads `metadata/gmeow-self.ttl`).

use std::collections::{BTreeMap, BTreeSet};

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
const GM_MAPPING_SET: &str = "https://blackcatinformatics.ca/gmeow/MappingSet";
const GM_SET_ID: &str = "https://blackcatinformatics.ca/gmeow/setId";
const GM_LICENSE: &str = "https://blackcatinformatics.ca/gmeow/license";
const GM_SET_COMMENT: &str = "https://blackcatinformatics.ca/gmeow/setComment";
const GM_SET_TRAILER: &str = "https://blackcatinformatics.ca/gmeow/setTrailer";

const DEFAULT_JUSTIFICATION: &str = "https://w3id.org/semapv/vocab/ManualMappingCuration";

/// The canonical SSSOM column order GMEOW writes. A label column only appears when at
/// least one row populates it; the [`SSSOM_ALWAYS`] columns are always present.
const SSSOM_ORDER: &[&str] = &[
    "subject_id",
    "subject_label",
    "predicate_id",
    "object_id",
    "object_label",
    "mapping_justification",
    "confidence",
    "comment",
];

/// The columns GMEOW always emits, even when blank for every row.
const SSSOM_ALWAYS: &[&str] = &[
    "subject_id",
    "predicate_id",
    "object_id",
    "mapping_justification",
    "confidence",
    "comment",
];

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

type RowsByFile = BTreeMap<String, Vec<Row>>;
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
    let sets = render_sets(&sources.mapping_sets, &rows_by_file, version, release_date)?;
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
            .push(Row {
                subject_id: sssom_id(&cell.subject, table),
                subject_label: cell.subject_label.clone(),
                predicate_id: sssom_id(&cell.predicate, table),
                object_id: sssom_id(&cell.obj, table),
                object_label: cell.object_label.clone(),
                mapping_justification: sssom_id(&justification, table),
                confidence: conf(cell.confidence),
                comment: cell.comment.clone(),
            });
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
                by_file.entry(file.to_owned()).or_default().push(Row {
                    subject_id: sssom_id(&subject, table),
                    subject_label: metadata.subject_label.clone(),
                    predicate_id: sssom_id(predicate, table),
                    object_id: sssom_id(&obj, table),
                    object_label: metadata.object_label.clone(),
                    mapping_justification: sssom_id(
                        metadata
                            .justification
                            .as_deref()
                            .unwrap_or(DEFAULT_JUSTIFICATION),
                        table,
                    ),
                    confidence: conf(binding.confidence),
                    comment: metadata.comment.clone(),
                });
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

fn extract_equivalences(view: &DslView, out: &mut Vec<EquivalenceCell>) {
    let _ = RDF_TYPE; // documented surface; subjects_of_type uses it internally.
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

/// One materialized SSSOM row: the eight named column cells.
struct Row {
    subject_id: String,
    subject_label: String,
    predicate_id: String,
    object_id: String,
    object_label: String,
    mapping_justification: String,
    confidence: String,
    comment: String,
}

impl Row {
    fn cell(&self, column: &str) -> &str {
        match column {
            "subject_id" => &self.subject_id,
            "subject_label" => &self.subject_label,
            "predicate_id" => &self.predicate_id,
            "object_id" => &self.object_id,
            "object_label" => &self.object_label,
            "mapping_justification" => &self.mapping_justification,
            "confidence" => &self.confidence,
            "comment" => &self.comment,
            _ => "",
        }
    }
}

fn render_sets(
    mapping_sets: &BTreeMap<String, MappingSet>,
    by_file: &BTreeMap<String, Vec<Row>>,
    version: &str,
    release_date: &str,
) -> gmeow_errors::Result<BTreeMap<String, String>> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (file, rows) in by_file {
        let meta = mapping_sets.get(file);
        out.insert(file.clone(), render_one(rows, meta, version, release_date)?);
    }
    Ok(out)
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

fn render_one(
    rows: &[Row],
    meta: Option<&MappingSet>,
    version: &str,
    release_date: &str,
) -> gmeow_errors::Result<String> {
    let columns: Vec<&str> = SSSOM_ORDER
        .iter()
        .copied()
        .filter(|c| SSSOM_ALWAYS.contains(c) || rows.iter().any(|r| !r.cell(c).is_empty()))
        .collect();

    let mut used: BTreeSet<String> = BTreeSet::new();
    for r in rows {
        for tok in [
            &r.subject_id,
            &r.predicate_id,
            &r.object_id,
            &r.mapping_justification,
        ] {
            if let Some((prefix, _)) = tok.split_once(':')
                && registry_iri(prefix).is_some()
            {
                used.insert(prefix.to_owned());
            }
        }
    }

    let mut lines = sssom_header(meta, &used, version, release_date);

    if let Some(meta) = meta
        && !meta.trailer.is_empty()
    {
        // Refused/deferred mappings kept IN the artifact: a second '#' makes each
        // trailer line a YAML-invisible comment.
        for line in meta.trailer.lines() {
            lines.push(format!("# #{}", line.strip_prefix('#').unwrap_or(line)));
        }
    }

    lines.push(columns.join("\t"));

    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        (&a.subject_id, &a.predicate_id, &a.object_id).cmp(&(
            &b.subject_id,
            &b.predicate_id,
            &b.object_id,
        ))
    });
    for r in sorted {
        let mut cells: Vec<&str> = Vec::with_capacity(columns.len());
        for c in &columns {
            let value = r.cell(c);
            check_tsv_cell(c, value)?;
            cells.push(value);
        }
        lines.push(cells.join("\t"));
    }

    let mut text = lines.join("\n");
    text.push('\n');
    Ok(text)
}

fn sssom_header(
    meta: Option<&MappingSet>,
    used: &BTreeSet<String>,
    version: &str,
    release_date: &str,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(meta) = meta
        && !meta.set_id.is_empty()
    {
        lines.push(format!("# mapping_set_id: {}", meta.set_id));
        lines.push(format!("# mapping_set_version: {version}"));
        lines.push(format!("# license: {}", meta.license));
    }
    lines.push("# mapping_tool: gmeow regenerate (mappings)".to_owned());
    lines.push(format!("# mapping_tool_version: {version}"));
    lines.push(format!("# mapping_date: {release_date}"));
    if let Some(meta) = meta
        && !meta.comment.is_empty()
    {
        let collapsed = collapse_whitespace(&meta.comment);
        lines.push(format!("# comment: {}", json_quote_ascii(&collapsed)));
    }
    lines.push("# curie_map:".to_owned());
    for prefix in used {
        if let Some(iri) = registry_iri(prefix) {
            lines.push(format!("#   {prefix}: {iri}"));
        }
    }
    lines
}

// ── Formatting helpers (byte-parity-critical; mirror Python `_conf`/`%g`) ─────────

fn conf(value: Option<f64>) -> String {
    let Some(v) = value else {
        return String::new();
    };
    if v == v.trunc() {
        format!("{v:.1}")
    } else {
        format_g(v)
    }
}

fn format_g(v: f64) -> String {
    const SIG: usize = 6;
    if v == 0.0 {
        return "0".to_owned();
    }
    let exponent = v.abs().log10().floor() as i32;
    if exponent < -4 || exponent >= SIG as i32 {
        let mantissa_prec = SIG.saturating_sub(1);
        let s = format!("{v:.*e}", mantissa_prec);
        return trim_scientific(&s);
    }
    let decimals = (SIG as i32 - 1 - exponent).max(0) as usize;
    let s = format!("{v:.*}", decimals);
    trim_fixed(&s)
}

fn trim_scientific(s: &str) -> String {
    let (mantissa, exp) = match s.split_once('e') {
        Some((m, e)) => (m, e),
        None => return s.to_owned(),
    };
    let mantissa = trim_fixed(mantissa);
    let (sign, digits) = match exp.strip_prefix('-') {
        Some(d) => ('-', d),
        None => ('+', exp.strip_prefix('+').unwrap_or(exp)),
    };
    let digits = if digits.len() < 2 {
        format!("{digits:0>2}")
    } else {
        digits.to_owned()
    };
    format!("{mantissa}e{sign}{digits}")
}

fn trim_fixed(s: &str) -> String {
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0');
        let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
        trimmed.to_owned()
    } else {
        s.to_owned()
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
        let make = |subj: &str, pred: &str, obj: &str, c: Option<f64>| Row {
            subject_id: sssom_id(subj, table),
            subject_label: String::new(),
            predicate_id: sssom_id(pred, table),
            object_id: sssom_id(obj, table),
            object_label: String::new(),
            mapping_justification: sssom_id(DEFAULT_JUSTIFICATION, table),
            confidence: conf(c),
            comment: String::new(),
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
        let text = render_one(&rows, Some(&meta), "0.1.0", "2026-06-03").unwrap();
        let expected = "\
# mapping_set_id: https://blackcatinformatics.ca/gmeow/mappings/demo
# mapping_set_version: 0.1.0
# license: https://creativecommons.org/licenses/by/4.0/
# mapping_tool: gmeow regenerate (mappings)
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
        let row = Row {
            subject_id: sssom_id(&format!("{GMEOW}Foo"), table),
            subject_label: "Foo label".to_owned(),
            predicate_id: sssom_id("http://www.w3.org/2004/02/skos/core#exactMatch", table),
            object_id: sssom_id(&format!("{GMEOW}Bar"), table),
            object_label: String::new(),
            mapping_justification: sssom_id(DEFAULT_JUSTIFICATION, table),
            confidence: conf(None),
            comment: String::new(),
        };
        let text = render_one(&[row], None, "0.1.0", "2026-06-03").unwrap();
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
    fn render_one_rejects_tab_in_cell() {
        let table = ns_to_prefix();
        let row = Row {
            subject_id: sssom_id(&format!("{GMEOW}Foo"), table),
            subject_label: "has\ttab".to_owned(),
            predicate_id: sssom_id("http://www.w3.org/2004/02/skos/core#exactMatch", table),
            object_id: sssom_id(&format!("{GMEOW}Bar"), table),
            object_label: String::new(),
            mapping_justification: sssom_id(DEFAULT_JUSTIFICATION, table),
            confidence: conf(None),
            comment: String::new(),
        };
        let err = render_one(&[row], None, "0.1.0", "2026-06-03")
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
