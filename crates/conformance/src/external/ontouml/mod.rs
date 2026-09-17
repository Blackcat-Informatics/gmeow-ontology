// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! OntoUML/UFO catalog ingestion for the foundation-discipline soundness oracle.
//!
//! [`parse_ontouml_model`] reads a FAIR OntoUML/UFO catalog Turtle serialization
//! into a typed model; [`lower_model`] projects that model onto the world-scoped,
//! all-IRI `logic:` stereotype ABox that `gmeow_logic::foundation::evaluate`
//! consumes, running the five native OntoUML disciplines
//! (StereotypeCardinality, FreeRole, MixIden, MixRig, RelComp) over it.
//!
//! Fragment boundary (no-optionality / hard-fail), mirroring the TPTP adapter:
//!
//! * A **malformed** serialization is an [`OntoumlError::Syntax`] — a hard parse
//!   failure.
//! * A **well-formed but out-of-fragment** construct (a stereotype outside the
//!   five disciplines, or a mediation whose ends cannot be resolved) is an
//!   [`OntoumlError::Unsupported`] — an honest capability gap, never a silent
//!   pass.
//!
//! The discipline-verdict comparator ([`compare`]) grades the fired discipline
//! set against a documented anti-pattern label: a documented anti-pattern that
//! fires is an agreement; one that does not is a corpus-only coverage gap; a
//! clean-control case that fires anything is a soundness false positive the
//! caller MUST hard-fail.

pub mod lower;
pub mod model;

pub use lower::{lower_model, lower_model_dataset};
pub use model::{
    Generalization, LOGIC_NS, Mediation, ONTOUML_NS, OntoClass, OntoumlError, OntoumlModel,
    parse_ontouml_model,
};

use gmeow_logic::foundation::{AntiRigidityPolicy, FoundationQuad, evaluate};
use gmeow_logic::store::WorldStore;

/// The `logic:violation` predicate IRI the foundation chase asserts one quad per
/// fired discipline on.
pub const VIOLATION_PRED: &str = "https://blackcatinformatics.ca/logic/violation";

/// Lower a model, load it into a fresh [`WorldStore`], and run the foundation
/// disciplines over it.
///
/// Returns the derived foundation quads, the lowered N-Quads text, and its quad
/// count. A lowering that produces non-loadable N-Quads is an internal defect, so
/// a native load or evaluate error is reported as [`OntoumlError::Syntax`] (a lowering
/// gap is [`OntoumlError::Unsupported`], raised earlier by [`lower_model`]).
pub fn lower_and_evaluate(
    model: &OntoumlModel,
    world_iri: &str,
    policy: AntiRigidityPolicy,
) -> Result<(Vec<FoundationQuad>, String, usize), OntoumlError> {
    let (quads, dataset) = evaluate_model(model, world_iri, policy)?;
    let count = dataset.quads().count();
    let text = purrdf::canonical_flat_nquads(&dataset)
        .map_err(|error| OntoumlError::Syntax(format!("render lowered model: {error}")))?;
    Ok((quads, text, count))
}

/// Evaluate the native lowered model without a serialization or parsing boundary.
///
/// # Errors
/// Malformed input and execution failure remain distinct from unsupported constructs.
pub fn evaluate_model(
    model: &OntoumlModel,
    world_iri: &str,
    policy: AntiRigidityPolicy,
) -> Result<(Vec<FoundationQuad>, std::sync::Arc<purrdf::RdfDataset>), OntoumlError> {
    let dataset = lower_model_dataset(model, world_iri)?;
    let store = WorldStore::from_dataset(&dataset)
        .map_err(|error| OntoumlError::Syntax(error.message().to_owned()))?;
    let quads = evaluate(&store, policy)
        .map_err(|error| OntoumlError::Syntax(error.message().to_owned()))?;
    Ok((quads, dataset))
}

/// The set of discipline local names fired as `logic:violation` in the derived
/// foundation quads (e.g. `"FreeRole"`, `"MixIden"`, `"StereotypeCardinality"`).
///
/// The `object` field is in N3 form (`<iri>`); the angle brackets are stripped
/// and the `logic:` namespace prefix removed to recover the bare discipline name.
pub fn fired_disciplines(quads: &[FoundationQuad]) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for q in quads {
        if q.predicate != VIOLATION_PRED {
            continue;
        }
        let obj = q
            .object
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(&q.object);
        if let Some(local) = obj.strip_prefix(LOGIC_NS) {
            out.insert(local.to_owned());
        }
    }
    out
}

/// The verdict comparing a documented anti-pattern label against the fired
/// discipline set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisciplineVerdict {
    /// The documented anti-pattern fired (or a clean-control case fired nothing).
    Agree,
    /// The documented anti-pattern was NOT reproduced by the native disciplines —
    /// a native coverage gap (the interesting corpus-only feed).
    CorpusOnly,
    /// A clean-control case fired a discipline — a soundness FALSE POSITIVE the
    /// caller MUST hard-fail.
    EngineOnly,
    /// Reserved for a capability gap surfaced upstream by a lowering failure (a
    /// construct the native fragment cannot carry); never produced by [`compare`]
    /// itself, which sees only successfully-lowered models.
    DlGap,
}

/// Compare a documented anti-pattern label against the fired discipline set.
///
/// * A documented label that appears in `fired` is an [`Agree`](DisciplineVerdict::Agree);
///   one that is absent is a [`CorpusOnly`](DisciplineVerdict::CorpusOnly) gap.
///   Extra disciplines fired *beyond* the documented one are a disclosed extra:
///   this comparator uses "contains" semantics and still returns `Agree` when the
///   documented label is present, regardless of the extras.
/// * A clean-control case (`documented == None`) that fires nothing is an
///   [`Agree`](DisciplineVerdict::Agree); one that fires *anything* is an
///   [`EngineOnly`](DisciplineVerdict::EngineOnly) soundness FALSE POSITIVE the
///   caller MUST treat as a hard failure (the clean-control soundness floor).
pub fn compare(
    documented: Option<&str>,
    fired: &std::collections::BTreeSet<String>,
) -> DisciplineVerdict {
    match documented {
        Some(label) => {
            if fired.contains(label) {
                DisciplineVerdict::Agree
            } else {
                DisciplineVerdict::CorpusOnly
            }
        }
        None => {
            if fired.is_empty() {
                DisciplineVerdict::Agree
            } else {
                DisciplineVerdict::EngineOnly
            }
        }
    }
}

/// The canonical native-verdict string for the divergence-ledger fold.
///
/// Returns the documented label when the verdict is [`Agree`](DisciplineVerdict::Agree)
/// (or `"clean"` for an agreeing clean-control case), else a sorted comma-join of
/// the fired disciplines (or `"clean"` when none fired). This string is the
/// `ExternalComparison.native` half against the documented label's `.published`,
/// so `gmeow_logic::reason::compare_external_corpus` classifies equal→Agree and
/// differ→CorpusOnly deterministically.
pub fn native_verdict_string(
    documented: Option<&str>,
    fired: &std::collections::BTreeSet<String>,
) -> String {
    match compare(documented, fired) {
        DisciplineVerdict::Agree => documented.map_or_else(|| "clean".to_owned(), str::to_owned),
        _ => {
            if fired.is_empty() {
                "clean".to_owned()
            } else {
                fired.iter().cloned().collect::<Vec<_>>().join(",")
            }
        }
    }
}

#[path = "mod.tests.rs"]
#[cfg(test)]
mod tests;
