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
//!   by construction), its confidence from `gmeow:confidence`, and its evidence strength
//!   from the justification band ([`evidence_strength_of_justification`]);
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
//! First the typed set is materialized; then the dialect gate/ledger paths are re-seated
//! onto it: alongside the [`CorrespondenceProgram`], the transpiler builds a
//! [`CorrespondenceLookup`] keyed by each cell's natural identity, and the SSSOM, EDOAL,
//! and SPARQL lowerings now CONSUME that materialized typed `(relation, morphism class,
//! morphism kind)` for their overclaim gate / ledger path instead of re-deriving the
//! relation inline — the materialized set is the single source of truth. (FnO never
//! derived a typed relation: it is `ValidationOnly` and has no overclaim gate, so it has
//! nothing to re-seat.) The four rendered artifacts stay byte-identical — the renderers
//! emit the authored predicate/relation token verbatim; only the GATE input moved.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use gmeow_errors::Diag;

use crate::ingest::DslView;
use crate::ir::{
    Correspondence, CorrespondenceRelation, LOGIC_NAMESPACE, MorphismClass, MorphismKind,
    PreservationKind,
};
use crate::projections::correspondence::CorrespondenceProgram;
use crate::projections::get_leg::{ProfileBinding, projections};
use crate::projections::sssom::{equivalence_cells, sssom_band};

/// The semapv justification under which a curator established a mapping — the
/// provenance-derived warrant the SSSOM cell carries. We map it to an
/// `evidenceStrength` band: a manually-curated mapping is a modest, non-zero warrant; a
/// lexical/structural heuristic would be weaker. An unknown/absent justification yields
/// `None` (never a fabricated number — the axis stays unset).
fn evidence_strength_of_justification(justification: Option<&str>) -> Option<f64> {
    let local = justification?.rsplit(['#', '/', ':']).next().unwrap_or("");
    Some(match local {
        // A human curator's deliberate assertion — a modest, non-zero warrant.
        "ManualMappingCuration" => 0.5,
        // Lexical/structural heuristics are weaker warrants than manual curation.
        "LexicalMatching" | "LexicalSimilarityThresholdMatching" => 0.3,
        "StructuralMatching" => 0.3,
        // An unrecognized justification: leave the axis unset rather than invent a value.
        _ => return None,
    })
}

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
/// disjoint key shapes (an equivalence triple vs a `(cell IRI, profile)` pair), so a
/// term-equivalence and a projection binding can never collide on a key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NaturalKey {
    /// A native alignment cell, keyed by its `(subject, predicate, object)` triple
    /// (one subject may align to several objects, so the whole triple is the identity).
    Equivalence {
        subject: String,
        predicate: String,
        obj: String,
    },
    /// A `gmeow:ProjectionMapping` per-profile binding, keyed by `(cell IRI, profile)`.
    Binding { cell_iri: String, profile: String },
}

/// A lookup from each authored cell's natural identity to its materialized typed
/// `(relation, morphism class, morphism kind)` — built once by the transpiler so the four
/// dialect lowerings CONSUME the materialized authority for their overclaim gate / ledger
/// path rather than re-deriving the relation inline. Keyed off the SAME
/// extraction the transpiler folds into the [`CorrespondenceProgram`], so the consumed
/// relation and the materialized typed node are identical by construction.
#[derive(Debug, Clone, Default)]
pub struct CorrespondenceLookup {
    by_key: BTreeMap<NaturalKey, TypedRelation>,
    /// Correspondence IRI → the profile of the `gmeow:ProjectionMapping` binding it was
    /// minted from. Only per-profile binding correspondences carry a profile (a
    /// native alignment cell is not profile-scoped and is absent here). Consumed by
    /// the mappings stage to pair a correspondence with its OWN per-binding get/put CONSTRUCT
    /// fragments for executed lens-law discharge — the per-profile UNION query is the wrong
    /// unit (a single UNION branch's law must be checked in isolation).
    binding_profiles: BTreeMap<String, String>,
}

impl CorrespondenceLookup {
    /// The materialized typed relation of a native alignment cell, keyed by its
    /// `(subject, predicate, object)` triple.
    ///
    /// # Errors
    ///
    /// HARD-fails if the cell has no materialized correspondence — every authored cell is
    /// transpiled, so a miss is a build invariant violation, never a silent skip
    /// (no-optionality).
    pub fn equivalence(
        &self,
        subject: &str,
        predicate: &str,
        obj: &str,
    ) -> gmeow_errors::Result<TypedRelation> {
        let key = NaturalKey::Equivalence {
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            obj: obj.to_owned(),
        };
        self.by_key.get(&key).copied().ok_or_else(|| {
            Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "no materialized correspondence for alignment cell \
                     ({subject}, {predicate}, {obj}) — every authored cell must be transpiled"
                ),
            })
        })
    }

    /// The materialized typed relation of a `gmeow:ProjectionMapping` per-profile binding,
    /// keyed by `(cell IRI, profile)`.
    ///
    /// # Errors
    ///
    /// HARD-fails if the binding has no materialized correspondence (no-optionality).
    pub fn binding(&self, cell_iri: &str, profile: &str) -> gmeow_errors::Result<TypedRelation> {
        let key = NaturalKey::Binding {
            cell_iri: cell_iri.to_owned(),
            profile: profile.to_owned(),
        };
        self.by_key.get(&key).copied().ok_or_else(|| {
            Diag::of_kind(crate::error::Correspondence {
                detail: format!(
                    "no materialized correspondence for ProjectionMapping binding \
                     ({cell_iri}, {profile}) — every authored binding must be transpiled"
                ),
            })
        })
    }

    /// Correspondence IRI → profile for every `gmeow:ProjectionMapping` binding
    /// correspondence (the map the mappings stage joins against the per-binding SPARQL
    /// fragments to discharge each correspondence's own lens law in isolation).
    pub fn binding_profiles(&self) -> &BTreeMap<String, String> {
        &self.binding_profiles
    }

    /// Build a lookup carrying a single `(cell IRI, profile)` binding entry — for the
    /// dialect lowerings' unit tests that construct a `ProfileBinding` directly (without a
    /// DSL store to transpile from). Production builds the lookup only via
    /// [`transpile_correspondences_indexed`].
    #[cfg(test)]
    pub(crate) fn for_binding_test(cell_iri: &str, profile: &str, typed: TypedRelation) -> Self {
        let mut by_key = BTreeMap::new();
        by_key.insert(
            NaturalKey::Binding {
                cell_iri: cell_iri.to_owned(),
                profile: profile.to_owned(),
            },
            typed,
        );
        Self {
            by_key,
            binding_profiles: BTreeMap::new(),
        }
    }
}

/// Transpile the authored `dsl/mappings/` cells into a typed [`CorrespondenceProgram`]:
/// one [`Correspondence`] per native alignment cell and one per
/// `gmeow:ProjectionMapping` per-profile binding. Thin wrapper over
/// [`transpile_correspondences_indexed`] for callers that need only the program.
///
/// # Errors
///
/// Propagates a malformed `gmeow:ProjectionMapping` (the get-leg parser's hard error) or a
/// rejected [`Correspondence::new`] invariant (a bad confidence/leg). Construction is
/// fail-hard: a malformed cell is a build failure, never a silently-dropped node.
pub fn transpile_correspondences(
    dsl_view: &DslView,
    onto_view: &DslView,
) -> gmeow_errors::Result<CorrespondenceProgram> {
    Ok(transpile_correspondences_indexed(dsl_view, onto_view)?.0)
}

/// Transpile the authored cells into BOTH the typed [`CorrespondenceProgram`] and the
/// [`CorrespondenceLookup`] keyed by each cell's natural identity. The lookup is the
/// single source of truth the four dialect lowerings consume for their overclaim gate /
/// ledger path — both products fold the SAME extraction + SAME shared
/// derivation, so the consumed relation and the materialized typed node agree by
/// construction.
///
/// `dsl_view` carries the alignment + mapping DSL; `onto_view` is accepted for symmetry
/// with the dialect lowerings (the EDOAL/SPARQL get-leg model reads it for ranges), so a
/// future enrichment of the materialized nodes from the ontology has the handle without a
/// signature change. The four dialect outputs' RENDERED bytes are unaffected (they still
/// emit the authored predicate/relation token verbatim).
///
/// # Errors
///
/// As [`transpile_correspondences`].
pub fn transpile_correspondences_indexed(
    dsl_view: &DslView,
    _onto_view: &DslView,
) -> gmeow_errors::Result<(CorrespondenceProgram, CorrespondenceLookup)> {
    let mut correspondences: Vec<Correspondence> = Vec::new();
    let mut by_key: BTreeMap<NaturalKey, TypedRelation> = BTreeMap::new();
    let mut binding_profiles: BTreeMap<String, String> = BTreeMap::new();

    // ── Native alignment cells (the SSSOM 1:1 band) ────────────────────────────────
    for cell in equivalence_cells(dsl_view)? {
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
        let evidence_strength = evidence_strength_of_justification(cell.justification.as_deref());
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
            cell.confidence,
            evidence_strength,
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
        if cell.grounding {
            corr = corr.as_grounding();
        }
        correspondences.push(corr);
        by_key.insert(
            NaturalKey::Equivalence {
                subject: cell.subject.clone(),
                predicate: cell.predicate.clone(),
                obj: cell.obj.clone(),
            },
            TypedRelation {
                relation,
                morphism_class,
                morphism_kind,
            },
        );
    }

    // ── gmeow:ProjectionMapping per-profile bindings (the EDOAL/SPARQL get leg) ─────
    for cell in projections(dsl_view)? {
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
            let (corr, typed) = correspondence_for_binding(&cell, binding)?;
            binding_profiles.insert(corr.iri.clone(), binding.profile.clone());
            correspondences.push(corr);
            by_key.insert(
                NaturalKey::Binding {
                    cell_iri: cell.iri.clone(),
                    profile: binding.profile.clone(),
                },
                typed,
            );
        }
    }

    // The frontend's preservation polarity for the lane: the alignment lowerings are a
    // sound under-approximation (they refuse the forced-equality reading), never exact.
    let program =
        CorrespondenceProgram::new(correspondences, Vec::new(), PreservationKind::SoundUnder);
    Ok((
        program,
        CorrespondenceLookup {
            by_key,
            binding_profiles,
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
    // The per-correspondence key folds (cell IRI, profile, target): one mapping cell has
    // one binding per profile, each its own correspondence.
    let key = format!("{}|{}|{target}", cell.iri, binding.profile);
    let iri = correspondence_iri("projection-mapping", &key);
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
        false,
        None,
        get_leg,
        put_leg,
        // An authored co-authored put-with-claim (`gmeow:ingestClaim`) becomes a real
        // `law_claims` entry the existing `p_has_law_claim` path round-trips; absent in the
        // committed corpus, so this is empty there.
        binding.ingest_claim.iter().cloned().collect(),
        binding.confidence,
        evidence_strength_of_justification(grounding.and_then(|g| g.justification.as_deref())),
        None,
        None,
        None,
        // Grounding correspondences author their own preservation boundary; ordinary
        // executable mappings inherit the lane-level SoundUnder polarity.
        preservation,
    )?;
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
