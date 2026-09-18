// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `dsl/mappings/` **correspondence frontend**: materialize ONE typed
//! [`Correspondence`] IR node per authored alignment cell, so the carrier holds a real
//! [`CorrespondenceProgram`] instead of an empty `LogicProgram.correspondences` whose
//! ledger is reconstructed ad hoc downstream.
//!
//! Two cell kinds feed the program, each reusing the SAME typed derivation its dialect
//! lowering already trusts — never a forked mapping:
//!
//! * a native alignment cell (the SSSOM 1:1 band): its relation + morphism class
//!   come from `sssom::sssom_band` (so the typed node and the rendered SSSOM TSV agree
//!   by construction), its confidence from `gmeow:confidence`, and its evidence
//!   retained independently from the full qualitative evidence identity;
//! * a `gmeow:ProjectionMapping` per-profile binding (the EDOAL/SPARQL get leg): its
//!   `(relation, morphism class, morphism kind)` come from [`ProfileBinding::lattice`],
//!   its get leg references the cell's pattern, and its confidence from the binding.
//!
//! The correspondence IRI is content-addressed (`sha256` of the cell's identifying
//! fields), so re-running the transpiler over the same corpus mints byte-identical node
//! identities — the program keys stably across builds and the cache boundary.
//!
//! # Scope
//!
//! One source admission builds the canonical program and its immutable
//! [`CorrespondenceAnalysis`]. SSSOM, EDOAL, SPARQL and FnO borrow the same parsed
//! cells and mapping patterns. Alignment and profile-binding indices preserve
//! each declaration's semantic identity; target gates consume its admitted
//! relation and morphism qualifiers. FnO projects signatures from the same
//! patterns and owns no second mapping parser.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use gmeow_errors::Diag;

use crate::ingest::DslView;
use crate::ir::{
    Correspondence, CorrespondenceRelation, LOGIC_NAMESPACE, MorphismClass, MorphismKind,
    PreservationKind,
};
use crate::projections::correspondence::CorrespondenceProgram;
use crate::projections::get_leg::{ProfileBinding, ProjectionCell, binding_key, projections};
use crate::projections::sssom::{equivalence_cells, sssom_band};

/// A content-addressed correspondence IRI under `LOGIC_NAMESPACE` for the cell keyed by
/// `key`. The `sha256(key)[:16]` digest mirrors the established content-IRI minting
/// (`projections::mod` / `rdf.rs`), so the identity is stable, collision-free, and
/// IRI-legal. `tag` segments the two cell kinds so a term-equivalence and a projection
/// binding can never collide on the same digest.
fn correspondence_iri(tag: &str, key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("{LOGIC_NAMESPACE}correspondence/{tag}/{hex}")
}

/// The content-addressed correspondence identity IRI for an alignment keyed by its
/// `(subject, predicate, object)` triple — the SAME digest scheme the transpiler mints for
/// a non-grounding term-equivalence. Exposed for consumers that record alignment provenance
/// (e.g. `gmeow:mappedFrom`) now that native alignment cells carry no bespoke `gmeow:eqXxx`
/// cell IRI. Stable, collision-free, and IRI-legal.
pub fn alignment_provenance_iri(subject: &str, predicate: &str, object: &str) -> String {
    correspondence_iri(
        "term-equivalence",
        &format!("{subject}|{predicate}|{object}"),
    )
}

/// Parse an optional `logic:` enum IRI authored on a mapping cell. The mapping SHACL
/// shape constrains these values too, but the compiler remains fail-closed when called
/// directly: a foreign namespace or unknown local name is never silently treated as the
/// default rung.
fn parse_logic_enum<T>(
    value: Option<&str>,
    owner: &str,
    field: &str,
    parse: impl FnOnce(&str) -> Option<T>,
) -> gmeow_errors::Result<Option<T>> {
    let Some(iri) = value else { return Ok(None) };
    let local = iri.strip_prefix(LOGIC_NAMESPACE).ok_or_else(|| {
        Diag::of_kind(crate::error::Correspondence {
            detail: format!("{owner} {field} must be a logic: IRI, found <{iri}>"),
        })
    })?;
    let parsed = parse(local).ok_or_else(|| {
        Diag::of_kind(crate::error::Correspondence {
            detail: format!("{owner} has unknown {field} <{iri}>"),
        })
    })?;
    Ok(Some(parsed))
}

/// The typed `(relation, morphism class, morphism kind)` envelope of one materialized
/// correspondence — the single source of truth a dialect lowering's overclaim gate and
/// ledger path now CONSUME, instead of re-deriving the relation inline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypedRelation {
    /// The typed relation on the alignment lattice (`logic:correspondenceRelation`).
    pub relation: CorrespondenceRelation,
    /// The rung on the ordered law-spine (`logic:morphismClass`).
    pub morphism_class: MorphismClass,
    /// The satisfaction-preserving / commitment-shifting qualifier (`logic:morphismKind`).
    pub morphism_kind: MorphismKind,
}

/// The natural identity of an authored alignment cell — the key under which a dialect
/// lowering looks up its materialized typed correspondence. The two cell kinds have
/// disjoint key shapes (an alignment declaration vs a complete source binding key), so a
/// term-equivalence and a projection binding can never collide on a key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NaturalKey {
    /// A native alignment assertion with its exact authored morphism qualifiers.
    /// Separate declarations about the same RDF assertion cannot overwrite each other.
    Equivalence {
        subject: String,
        predicate: String,
        obj: String,
        morphism_class: Option<String>,
        morphism_kind: Option<String>,
        preservation: Option<String>,
        grounding: bool,
    },
    /// A `gmeow:ProjectionMapping` per-profile binding, keyed by its complete source/pattern/profile/binding semantics.
    Binding { semantic_key: String },
}

impl NaturalKey {
    fn alignment(cell: &super::sssom::EquivalenceCell) -> Self {
        Self::Equivalence {
            subject: cell.subject.clone(),
            predicate: cell.predicate.clone(),
            obj: cell.obj.clone(),
            morphism_class: cell.morphism_class.clone(),
            morphism_kind: cell.morphism_kind.clone(),
            preservation: cell.preservation.clone(),
            grounding: cell.grounding,
        }
    }
}

/// One immutable mapping analysis: admitted source cells and their materialized typed
/// `(relation, morphism class, morphism kind)` — built once by the transpiler so the four
/// dialect lowerings CONSUME the materialized authority for their overclaim gate / ledger
/// path rather than re-deriving the relation inline. Keyed off the SAME
/// extraction the transpiler folds into the [`CorrespondenceProgram`], so the consumed
/// relation and the materialized typed node are identical by construction. Target
/// emitters borrow these cells; they never re-extract mapping patterns from RDF.
#[derive(Debug)]
pub struct CorrespondenceAnalysis {
    alignment_cells: Vec<super::sssom::EquivalenceCell>,
    projection_cells: Vec<ProjectionCell>,
    by_key: BTreeMap<NaturalKey, TypedRelation>,
    /// Correspondence IRI → the complete semantic key of the `gmeow:ProjectionMapping` binding it was
    /// minted from. Only per-profile binding correspondences carry a profile (a
    /// native alignment cell is not profile-scoped and is absent here). Consumed by
    /// the mappings stage to pair a correspondence with its OWN per-binding get/put CONSTRUCT
    /// programs for executed lens-law discharge — the per-profile UNION query is the wrong
    /// unit (a single UNION branch's law must be checked in isolation).
    binding_keys: BTreeMap<String, String>,
}

impl CorrespondenceAnalysis {
    /// The exact native alignment cells from which the typed program was admitted.
    pub fn alignment_cells(&self) -> &[super::sssom::EquivalenceCell] {
        &self.alignment_cells
    }

    /// The exact mapping patterns and bindings shared by every target lowering.
    pub fn projection_cells(&self) -> &[ProjectionCell] {
        &self.projection_cells
    }

    /// The materialized typed relation of this exact alignment declaration,
    /// including its authored morphism qualifiers. File membership is independent.
    ///
    /// # Errors
    ///
    /// HARD-fails if the cell has no materialized correspondence — every authored cell is
    /// transpiled, so a miss is a build invariant violation, never a silent skip
    /// (no-optionality).
    pub fn equivalence(
        &self,
        cell: &super::sssom::EquivalenceCell,
    ) -> gmeow_errors::Result<TypedRelation> {
        let key = NaturalKey::alignment(cell);
        self.by_key.get(&key).copied().ok_or_else(|| {
            Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "no materialized correspondence for alignment cell \
                     ({}, {}, {}) — every authored cell must be transpiled",
                    cell.subject, cell.predicate, cell.obj,
                ),
            })
        })
    }

    /// The materialized typed relation of a `gmeow:ProjectionMapping` per-profile binding,
    /// keyed by its complete source/pattern/profile/binding semantics.
    ///
    /// # Errors
    ///
    /// HARD-fails if the binding has no materialized correspondence (no-optionality).
    pub fn binding(
        &self,
        cell: &ProjectionCell,
        binding: &ProfileBinding,
    ) -> gmeow_errors::Result<TypedRelation> {
        let semantic_key = binding_key(cell, binding);
        let key = NaturalKey::Binding {
            semantic_key: semantic_key.clone(),
        };
        self.by_key.get(&key).copied().ok_or_else(|| {
            Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "no materialized correspondence for ProjectionMapping binding \
                     ({semantic_key}) — every authored binding must be transpiled"
                ),
            })
        })
    }

    /// Correspondence IRI → complete semantic key for every `gmeow:ProjectionMapping` binding
    /// correspondence (the map the mappings stage joins against the per-binding SPARQL
    /// programs to discharge each correspondence's own lens law in isolation).
    pub fn binding_keys(&self) -> &BTreeMap<String, String> {
        &self.binding_keys
    }
}

/// Transpile the authored cells into BOTH the typed [`CorrespondenceProgram`] and the
/// [`CorrespondenceAnalysis`] keyed by each cell's natural identity. The lookup is the
/// single source of truth the four dialect lowerings consume for their overclaim gate /
/// ledger path — both products fold the SAME extraction + SAME shared
/// derivation, so the consumed relation and the materialized typed node agree by
/// construction.
///
/// The source view supplies the complete authored alignment and mapping inputs.
///
/// # Errors
/// Refuses malformed source patterns, unresolved correspondence fields, conflicting
/// declarations and invalid typed coordinates before publishing either product.
pub fn transpile_correspondences_indexed(
    dsl_view: &DslView,
) -> gmeow_errors::Result<(CorrespondenceProgram, CorrespondenceAnalysis)> {
    let mut correspondences: Vec<Correspondence> = Vec::new();
    let mut by_key: BTreeMap<NaturalKey, TypedRelation> = BTreeMap::new();
    let mut binding_keys: BTreeMap<String, String> = BTreeMap::new();
    // Two authored cells that mint the SAME content-addressed correspondence IRI must agree on
    // the SEMANTIC identity of the fact: confidence, justification, and endpoints. The IRI
    // folds in (subject, predicate, object) + morphism metadata, so a cell that additionally
    // diverges on confidence/justification/endpoints would otherwise be resolved by silent
    // last-write-wins — a MAXIMAL-INFORMATION-FLOW violation. `sssom_file` is deliberately
    // EXCLUDED from the signature: the SSSOM projection emits one row per cell keyed by its
    // file (`build_rows_and_ledger`), so one fact may legitimately belong to several thematic
    // SSSOM sets; the typed correspondence (which carries no file) collapses those to one node.
    // Map each minted IRI to (semantic signature, first sssom file) and fail closed on a clash.
    let mut seen_correspondences: BTreeMap<String, (String, String)> = BTreeMap::new();

    let alignment_cells = equivalence_cells(dsl_view)?;
    let projection_cells = projections(dsl_view)?;

    // ── Native alignment cells (the SSSOM 1:1 band) ────────────────────────────────
    for cell in &alignment_cells {
        // Relation + morphism class from the SAME band the SSSOM ledger gate uses.
        let (relation, derived_class) = sssom_band(&cell.predicate);
        let authored_class = parse_logic_enum(
            cell.morphism_class.as_deref(),
            "alignment cell",
            "logic:morphismClass",
            MorphismClass::from_local,
        )?;
        let authored_kind = parse_logic_enum(
            cell.morphism_kind.as_deref(),
            "alignment cell",
            "logic:morphismKind",
            MorphismKind::from_local,
        )?;
        let preservation = parse_logic_enum(
            cell.preservation.as_deref(),
            "alignment cell",
            "logic:preservationKind",
            PreservationKind::from_local,
        )?;
        if cell.grounding && cell.justification.is_none() {
            return Err(Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "grounding alignment cell ({}, {}, {}) must explicitly author \
                     gmeow:justification",
                    cell.subject, cell.predicate, cell.obj
                ),
            }));
        }
        if cell.grounding
            && (authored_class.is_none()
                || authored_kind.is_none()
                || preservation.is_none()
                || cell.source_endpoint.is_none()
                || cell.target_endpoint.is_none())
        {
            return Err(Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "grounding alignment cell ({}, {}, {}) must explicitly author \
                     logic:sourceEndpoint, logic:targetEndpoint, logic:morphismClass, \
                     logic:morphismKind, and logic:preservationKind",
                    cell.subject, cell.predicate, cell.obj
                ),
            }));
        }
        if cell.grounding
            && (cell.source_endpoint.as_deref() != Some(cell.subject.as_str())
                || cell.target_endpoint.as_deref() != Some(cell.obj.as_str()))
        {
            return Err(Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "grounding alignment cell ({}, {}, {}) endpoints must agree with \
                     the match subject and object",
                    cell.subject, cell.predicate, cell.obj
                ),
            }));
        }
        let morphism_class = authored_class.unwrap_or(derived_class);
        // The ordinary 1:1 SSSOM band defaults to a satisfaction-preserving lens; a
        // grounding bridge can explicitly replace that with CommitmentShiftingBridge.
        let morphism_kind = authored_kind.unwrap_or(MorphismKind::InstitutionMorphism);
        if cell.grounding
            && ((morphism_class == MorphismClass::BridgeView)
                != (morphism_kind == MorphismKind::CommitmentShiftingBridge))
        {
            return Err(Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "grounding alignment cell ({}, {}, {}) must pair logic:BridgeView with \
                     logic:CommitmentShiftingBridge (and only that pair)",
                    cell.subject, cell.predicate, cell.obj
                ),
            }));
        }
        // The per-correspondence key folds (subject, predicate, object) — one subject may
        // align to several objects, so the triple (not just the subject) is the identity.
        let authored_key = if cell.morphism_class.is_some()
            || cell.morphism_kind.is_some()
            || cell.preservation.is_some()
            || cell.grounding
        {
            format!(
                "|class={}|kind={}|pres={}|grounding={}",
                morphism_class.as_str(),
                morphism_kind.as_str(),
                preservation.map(|p| p.as_str()).unwrap_or(""),
                cell.grounding,
            )
        } else {
            String::new()
        };
        let key = format!(
            "{}|{}|{}{}",
            cell.subject, cell.predicate, cell.obj, authored_key
        );
        let iri = correspondence_iri("term-equivalence", &key);
        // Fail closed on a semantically divergent duplicate; collapse a redundant restatement.
        let signature = format!(
            "conf={:?}|just={:?}|src={:?}|tgt={:?}|loss={:?}",
            cell.confidence,
            cell.justification,
            cell.source_endpoint,
            cell.target_endpoint,
            cell.lossy_drops,
        );
        // Every admitted source spelling needs its typed relation, including
        // restatements whose program node is already present in another set.
        by_key.insert(
            NaturalKey::alignment(cell),
            TypedRelation {
                relation,
                morphism_class,
                morphism_kind,
            },
        );
        match seen_correspondences.get(&iri) {
            Some((prev_signature, prev_file)) if *prev_signature != signature => {
                return Err(Diag::of_kind(crate::error::Correspondence {
                    detail: format!(
                        "divergent duplicate alignment cell: ({}, {}, {}) is authored more than \
                         once with conflicting metadata \
                         (confidence/justification/endpoints/loss evidence) — \
                         first in '{}', again in '{}'. Both mint the same content-addressed \
                         correspondence identity, so one would be silently dropped; reconcile \
                         them to a single canonical value.",
                        cell.subject, cell.predicate, cell.obj, prev_file, cell.sssom_file,
                    ),
                }));
            }
            // A semantically identical restatement (possibly in another SSSOM set) is already
            // materialized — skip the redundant typed node; the per-cell SSSOM emission keeps
            // its own file membership.
            Some(_) => continue,
            None => {
                seen_correspondences.insert(iri.clone(), (signature, cell.sssom_file.clone()));
            }
        }
        let mut corr = Correspondence::new(
            iri,
            relation,
            morphism_class,
            morphism_kind,
            false,
            None,
            // The SSSOM 1:1 band carries only (subject, predicate, object) + confidence +
            // justification; it drops the get/put leg-program structure (that is what makes
            // its ledger row a `SoundUnder` drop), so the typed node leaves the legs unset.
            None,
            None,
            Vec::new(),
            cell.confidence.clone(),
            None,
            None,
            None,
            // Unindexed cells are scoped to the unspecified standpoint (unspecified, not
            // universal): `gmeow:accordingTo` stays unset.
            None,
            // Ordinary cells inherit the lane polarity; grounding cells author their own
            // preservation judgment explicitly.
            preservation,
        )?
        .with_endpoints(
            cell.source_endpoint
                .clone()
                .unwrap_or_else(|| cell.subject.clone()),
            cell.target_endpoint
                .clone()
                .unwrap_or_else(|| cell.obj.clone()),
        )?;
        corr = corr.with_axis_evidence(crate::ir::AxisEvidence::new(
            cell.justification.iter().cloned().collect(),
            None,
            None,
        )?);
        corr = corr.with_loss_evidence(cell.lossy_drops.clone())?;
        if cell.grounding {
            corr = corr.as_grounding();
        }
        correspondences.push(corr);
    }

    // ── gmeow:ProjectionMapping per-profile bindings (the EDOAL/SPARQL get leg) ─────
    for cell in &projection_cells {
        if cell.grounding.is_some() && cell.bindings.len() != 1 {
            return Err(Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "grounding ProjectionMapping {} must carry exactly one gmeow:hasBinding; \
                     found {}",
                    cell.iri,
                    cell.bindings.len()
                ),
            }));
        }
        for binding in &cell.bindings {
            let (corr, typed) = correspondence_for_binding(cell, binding)?;
            binding_keys.insert(corr.iri.clone(), binding_key(cell, binding));
            correspondences.push(corr);
            by_key.insert(
                NaturalKey::Binding {
                    semantic_key: binding_key(cell, binding),
                },
                typed,
            );
        }
    }

    // The frontend's preservation polarity for the lane: the alignment lowerings are a
    // sound under-approximation (they refuse the forced-equality reading), never exact.
    let program = CorrespondenceProgram::new(correspondences, PreservationKind::SoundUnder);
    Ok((
        program,
        CorrespondenceAnalysis {
            alignment_cells,
            projection_cells,
            by_key,
            binding_keys,
        },
    ))
}

/// Materialize the typed [`Correspondence`] for one `gmeow:ProjectionMapping` profile
/// binding, reusing [`ProfileBinding::lattice`] for the relation/class/kind triple — and
/// return that [`TypedRelation`] alongside it (computed once) so the lookup the dialect
/// gates consume and the materialized node share one derivation.
fn correspondence_for_binding(
    cell: &crate::projections::get_leg::ProjectionCell,
    binding: &ProfileBinding,
) -> gmeow_errors::Result<(Correspondence, TypedRelation)> {
    let (relation, derived_class, derived_kind) = binding.lattice();
    let grounding = cell.grounding.as_ref();
    let authored_class = parse_logic_enum(
        grounding.and_then(|g| g.morphism_class.as_deref()),
        "ProjectionMapping",
        "logic:morphismClass",
        MorphismClass::from_local,
    )?;
    let authored_kind = parse_logic_enum(
        grounding.and_then(|g| g.morphism_kind.as_deref()),
        "ProjectionMapping",
        "logic:morphismKind",
        MorphismKind::from_local,
    )?;
    let preservation = parse_logic_enum(
        grounding.and_then(|g| g.preservation.as_deref()),
        "ProjectionMapping",
        "logic:preservationKind",
        PreservationKind::from_local,
    )?;
    if let Some(grounding) = grounding
        && (grounding.justification.is_none()
            || authored_class.is_none()
            || authored_kind.is_none()
            || preservation.is_none()
            || grounding.source_endpoint.is_none()
            || grounding.target_endpoint.is_none())
    {
        return Err(Diag::of_kind(crate::error::Correspondence {
            detail: format!(
                "grounding ProjectionMapping {} must explicitly author gmeow:justification, \
                 logic:sourceEndpoint, logic:targetEndpoint, logic:morphismClass, \
                 logic:morphismKind, and logic:preservationKind",
                cell.iri
            ),
        }));
    }
    let morphism_class = authored_class.unwrap_or(derived_class);
    let morphism_kind = authored_kind.unwrap_or(derived_kind);
    if grounding.is_some()
        && ((morphism_class == MorphismClass::BridgeView)
            != (morphism_kind == MorphismKind::CommitmentShiftingBridge))
    {
        return Err(Diag::of_kind(crate::error::Correspondence {
            detail: format!(
                "grounding ProjectionMapping {} must pair logic:BridgeView with \
                 logic:CommitmentShiftingBridge (and only that pair)",
                cell.iri
            ),
        }));
    }
    if grounding.is_some()
        && morphism_class == MorphismClass::BridgeView
        && relation == CorrespondenceRelation::Equiv
    {
        return Err(Diag::of_kind(crate::error::Correspondence {
            detail: format!(
                "grounding ProjectionMapping {} is a commitment-shifting BridgeView and must \
                 not declare an equivalence binding relation",
                cell.iri
            ),
        }));
    }
    // The per-profile target IRI the binding projects onto (predicate, class, or EDOAL
    // target). A grounding binding names EXACTLY one of these target forms; otherwise its
    // authored targetEndpoint is ambiguous (or points at no executable target at all).
    let binding_targets = [
        binding.to_predicate.as_deref(),
        binding.to_class.as_deref(),
        binding.edoal_target.as_deref(),
    ];
    let target_count = binding_targets
        .iter()
        .filter(|target| target.is_some())
        .count();
    if grounding.is_some() && target_count != 1 {
        return Err(Diag::of_kind(crate::error::Correspondence {
            detail: format!(
                "grounding ProjectionMapping {} single binding must carry exactly one of \
                 gmeow:toPredicate, gmeow:toClass, or gmeow:edoalTarget; found {target_count}",
                cell.iri
            ),
        }));
    }
    let target = binding_targets.into_iter().flatten().next().unwrap_or("");
    if let Some(grounding) = grounding
        && grounding.target_endpoint.as_deref() != Some(target)
    {
        return Err(Diag::of_kind(crate::error::Correspondence {
            detail: format!(
                "grounding ProjectionMapping {} targetEndpoint must equal its single binding \
                 target <{}>",
                cell.iri, target
            ),
        }));
    }
    // One cell may have multiple semantically distinct bindings in the same profile.
    // Use the exact shared identity used by relation lookup, report and native legs.
    let key = binding_key(cell, binding);
    let digest = key
        .rsplit("binding=")
        .next()
        .expect("binding key has a digest");
    let iri = format!("{LOGIC_NAMESPACE}correspondence/projection-mapping/{digest}");
    // The get leg references the pattern-bearing mapping cell (an IRI node, the acquired
    // source pattern); the put leg is the per-profile target IRI it projects onto, when
    // the binding names one. Both are absolute IRIs (the pattern's SPARQL-variable anchor
    // is NOT an IRI, so it is never used as a leg).
    let get_leg = Some(cell.iri.clone());
    let put_leg = (!target.trim().is_empty()).then(|| target.to_owned());
    let mut corr = Correspondence::new(
        iri,
        relation,
        morphism_class,
        morphism_kind,
        binding.mnemomorphic,
        None,
        get_leg,
        put_leg,
        // An authored co-authored put-with-claim (`gmeow:ingestClaim`) becomes a real
        // `law_claims` entry the existing `p_has_law_claim` path round-trips; absent in the
        // committed corpus, so this is empty there.
        binding.ingest_claim.iter().cloned().collect(),
        binding.confidence.clone(),
        None,
        None,
        None,
        None,
        // Grounding correspondences author their own preservation boundary; ordinary
        // executable mappings inherit the lane-level SoundUnder polarity.
        preservation,
    )?;
    corr = corr.with_axis_evidence(crate::ir::AxisEvidence::new(
        grounding
            .and_then(|g| g.justification.as_ref())
            .into_iter()
            .cloned()
            .collect(),
        None,
        None,
    )?);
    if let Some(grounding) = grounding {
        corr = corr
            .with_endpoints(
                grounding.source_endpoint.clone().expect("checked above"),
                grounding.target_endpoint.clone().expect("checked above"),
            )?
            .as_grounding();
    }
    Ok((
        corr,
        TypedRelation {
            relation,
            morphism_class,
            morphism_kind,
        },
    ))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_support;
