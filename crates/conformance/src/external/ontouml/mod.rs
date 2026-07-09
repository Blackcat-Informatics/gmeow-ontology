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

pub use lower::lower_model;
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
/// a load or evaluate error is reported as [`OntoumlError::Syntax`] (a lowering
/// gap is [`OntoumlError::Unsupported`], raised earlier by [`lower_model`]).
pub fn lower_and_evaluate(
    model: &OntoumlModel,
    world_iri: &str,
    policy: AntiRigidityPolicy,
) -> Result<(Vec<FoundationQuad>, String, usize), OntoumlError> {
    let (nq, count) = lower_model(model, world_iri)?;
    let store = WorldStore::new();
    store
        .load_nquads(&nq)
        .map_err(|e| OntoumlError::Syntax(e.message().to_owned()))?;
    let quads =
        evaluate(&store, policy).map_err(|e| OntoumlError::Syntax(e.message().to_owned()))?;
    Ok((quads, nq, count))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> std::collections::BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn agree_when_documented_fired() {
        let fired = set(&["FreeRole", "MixIden"]);
        assert_eq!(compare(Some("FreeRole"), &fired), DisciplineVerdict::Agree);
        assert_eq!(native_verdict_string(Some("FreeRole"), &fired), "FreeRole");
    }

    #[test]
    fn corpus_only_when_documented_missed() {
        let fired = set(&["MixIden"]);
        assert_eq!(
            compare(Some("FreeRole"), &fired),
            DisciplineVerdict::CorpusOnly
        );
        assert_eq!(native_verdict_string(Some("FreeRole"), &fired), "MixIden");
    }

    #[test]
    fn agree_when_clean_fires_nothing() {
        let fired = set(&[]);
        assert_eq!(compare(None, &fired), DisciplineVerdict::Agree);
        assert_eq!(native_verdict_string(None, &fired), "clean");
    }

    #[test]
    fn engine_only_when_clean_fires_something() {
        let fired = set(&["RelComp"]);
        assert_eq!(compare(None, &fired), DisciplineVerdict::EngineOnly);
        assert_eq!(native_verdict_string(None, &fired), "RelComp");
    }

    #[test]
    fn free_role_model_fires_free_role_end_to_end() {
        // A lone role class (no rigid ancestor) is the classic FreeRole anti-pattern.
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Wanderer a ontouml:Class ; ontouml:stereotype ontouml:role .\n";
        let model = parse_ontouml_model(src, None).unwrap();
        let (quads, _nq, _count) = lower_and_evaluate(
            &model,
            "https://example.org/onto/schema",
            AntiRigidityPolicy::SchemaOnly,
        )
        .unwrap();
        let fired = fired_disciplines(&quads);
        assert!(fired.contains("FreeRole"), "fired={fired:?}");
        assert_eq!(compare(Some("FreeRole"), &fired), DisciplineVerdict::Agree);
    }

    #[test]
    fn functional_relator_fires_relcomp_end_to_end() {
        // A concrete relator mediating a single functional relatum is the RelComp
        // anti-pattern (a relator must mediate at least two entities).
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relatorEnd ex:Marriage ; ontouml:mediatedEnd ex:Spouse ;\n\
    ontouml:functionalMediation true .\n";
        let model = parse_ontouml_model(src, None).unwrap();
        let (quads, _nq, _count) = lower_and_evaluate(
            &model,
            "https://example.org/onto/schema",
            AntiRigidityPolicy::SchemaOnly,
        )
        .unwrap();
        let fired = fired_disciplines(&quads);
        assert!(fired.contains("RelComp"), "fired={fired:?}");
    }

    #[test]
    fn two_ended_relator_does_not_fire_relcomp() {
        // A relator mediating two distinct entities satisfies the discipline.
        let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Employment a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Employee a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:Employer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relatorEnd ex:Employment ; ontouml:mediatedEnd ex:Employee , ex:Employer .\n";
        let model = parse_ontouml_model(src, None).unwrap();
        let (quads, _nq, _count) = lower_and_evaluate(
            &model,
            "https://example.org/onto/schema",
            AntiRigidityPolicy::SchemaOnly,
        )
        .unwrap();
        let fired = fired_disciplines(&quads);
        assert!(!fired.contains("RelComp"), "fired={fired:?}");
    }
}
