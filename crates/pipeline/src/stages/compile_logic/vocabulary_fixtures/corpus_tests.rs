// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Grade every vocabulary contract from one authenticated producer observation.

use super::{CHANNEL, Observation};
use gmeow_logic_compile::ir::*;
use std::collections::BTreeSet;

type VocabularyContract = (&'static str, fn(&Observation));

#[test]
fn corpus_logic_vocabulary_surface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = crate::fixture::authenticated_artifact(&root, "stage-compile-logic", CHANNEL)
        .expect("producer-authenticated native vocabulary observation");
    let observation: Observation =
        serde_json::from_slice(&bytes).expect("decode vocabulary observation");
    assert_eq!(observation.source, super::super::SOURCE_PATH);
    let contracts: [VocabularyContract; 13] = [
        (
            "semantic_profile_ids_match_module_ttl",
            semantic_profile_ids_match_module_ttl,
        ),
        (
            "procedural_preset_carries_procedural_execution_facet",
            procedural_preset_carries_procedural_execution_facet,
        ),
        (
            "compatibility_rule_ids_match_module_ttl",
            compatibility_rule_ids_match_module_ttl,
        ),
        (
            "preservation_kind_values_match_module_ttl",
            preservation_kind_values_match_module_ttl,
        ),
        (
            "node_kind_values_match_module_ttl",
            node_kind_values_match_module_ttl,
        ),
        (
            "formula_shape_values_match_module_ttl",
            formula_shape_values_match_module_ttl,
        ),
        (
            "correspondence_relation_values_match_module_ttl",
            correspondence_relation_values_match_module_ttl,
        ),
        (
            "morphism_class_values_match_module_ttl",
            morphism_class_values_match_module_ttl,
        ),
        (
            "morphism_kind_values_match_module_ttl",
            morphism_kind_values_match_module_ttl,
        ),
        (
            "determinacy_values_match_module_ttl",
            determinacy_values_match_module_ttl,
        ),
        (
            "correspondence_law_values_match_module_ttl",
            correspondence_law_values_match_module_ttl,
        ),
        (
            "discharge_verdict_values_match_module_ttl",
            discharge_verdict_values_match_module_ttl,
        ),
        (
            "discharge_condition_values_match_module_ttl",
            discharge_condition_values_match_module_ttl,
        ),
    ];
    let mut failures = Vec::new();
    for (name, check) in contracts {
        // Grade every named contract even when an earlier assertion fails. The
        // immutable observation is authenticated only once for this process.
        if std::panic::catch_unwind(|| check(&observation)).is_err() {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "vocabulary contracts failed: {}",
        failures.join(", ")
    );
}

fn members(observation: &Observation, class: &str) -> BTreeSet<String> {
    observation
        .members
        .get(class)
        .expect("producer selected vocabulary class")
        .as_ref()
        .unwrap_or_else(|error| panic!("{class}: {error}"))
        .iter()
        .map(|iri| {
            iri.strip_prefix(LOGIC_NAMESPACE)
                .unwrap_or_else(|| panic!("unexpected non-logic member of {class}: {iri}"))
                .to_owned()
        })
        .collect()
}

fn assert_members(observation: &Observation, rust: &[&str], class: &str) {
    let expected: BTreeSet<_> = rust.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(
        expected,
        members(observation, class),
        "{class} must match its native declaration inventory"
    );
}

fn semantic_profile_ids_match_module_ttl(observation: &Observation) {
    let got: std::collections::BTreeSet<&str> = [
        SemanticProfileId::PositiveHorn,
        SemanticProfileId::StratifiedNaf,
        SemanticProfileId::WellFounded,
        SemanticProfileId::StableModel,
        SemanticProfileId::ProceduralProlog,
        SemanticProfileId::Probabilistic,
    ]
    .iter()
    .map(|p| p.as_str())
    .collect();

    // The native declaration inventory must match every Rust preset.
    let from_ttl = members(observation, "ReasoningPreset");

    let from_rust: std::collections::BTreeSet<&str> = got.iter().copied().collect();
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        from_rust, from_ttl_refs,
        "SemanticProfileId enum must match the logic:ReasoningPreset individuals in module.ttl"
    );

    // Round-trip through from_local.
    for p in &got {
        assert_eq!(SemanticProfileId::from_local(p).unwrap().as_str(), *p);
    }
}

fn procedural_preset_carries_procedural_execution_facet(observation: &Observation) {
    let carries = observation
        .preset_facets
        .as_ref()
        .expect("native preset facets");
    for id in [
        SemanticProfileId::PositiveHorn,
        SemanticProfileId::StratifiedNaf,
        SemanticProfileId::WellFounded,
        SemanticProfileId::StableModel,
        SemanticProfileId::ProceduralProlog,
        SemanticProfileId::Probabilistic,
    ] {
        let in_ttl = carries
            .get(&id.iri())
            .expect("producer selected every preset")
            .contains(&format!("{LOGIC_NAMESPACE}{PROCEDURAL_EXECUTION_FACET}"));
        assert_eq!(
            in_ttl,
            id.permits_procedural_execution(),
            "preset {} procedural-execution permission must agree between module.ttl \
             ProceduralExecution \
             bundle ({in_ttl}) and SemanticProfileId::permits_procedural_execution ({})",
            id.as_str(),
            id.permits_procedural_execution()
        );
    }
}

fn compatibility_rule_ids_match_module_ttl(observation: &Observation) {
    // The Rust authority (compat.rs ALL_RULE_IDS) and the ontology surface
    // (logic:CompatibilityRule individuals in module.ttl) must never diverge:
    // every rust rule id is an individual local name and vice versa.
    use gmeow_logic_compile::compat::ALL_RULE_IDS;

    let from_ttl = members(observation, "CompatibilityRule");

    let from_rust: std::collections::BTreeSet<String> =
        ALL_RULE_IDS.iter().map(|s| (*s).to_owned()).collect();

    assert_eq!(
        from_rust, from_ttl,
        "Rust compat rule ids must match logic:CompatibilityRule individuals in module.ttl"
    );
}

fn preservation_kind_values_match_module_ttl(observation: &Observation) {
    let got: std::collections::BTreeSet<&str> = [
        PreservationKind::Exact,
        PreservationKind::SoundUnder,
        PreservationKind::CompleteOver,
        PreservationKind::ValidationOnly,
        PreservationKind::InconsistencyPreserving,
        PreservationKind::InconsistencyReflecting,
        PreservationKind::Unsupported,
    ]
    .iter()
    .map(|k| k.as_str())
    .collect();
    let expected: std::collections::BTreeSet<&str> = [
        "ExactPreservation",
        "SoundUnderApproximation",
        "CompleteOverApproximation",
        "ValidationOnly",
        "InconsistencyPreserving",
        "InconsistencyReflecting",
        "Unsupported",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, expected);

    // The seven enum values must be EXACTLY the logic:PreservationKind individuals
    // declared in module.ttl — so the new Unsupported floor is pinned to the ontology.
    let from_ttl = members(observation, "PreservationKind");
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        got, from_ttl_refs,
        "PreservationKind enum must match the logic:PreservationKind individuals in module.ttl"
    );
    assert!(
        from_ttl.contains("Unsupported"),
        "the Unsupported floor is declared"
    );
    for kind in PreservationKind::ALL {
        assert_eq!(PreservationKind::from_local(kind.as_str()), Some(kind));
    }
    assert_eq!(PreservationKind::from_local("NotAPreservationKind"), None);
}

fn node_kind_values_match_module_ttl(observation: &Observation) {
    let from_rust: std::collections::BTreeSet<&str> = [
        NodeKind::ObjectLevelFormula,
        NodeKind::MetaLevelFormula,
        NodeKind::Constraint,
        NodeKind::DerivationRule,
        NodeKind::Query,
        NodeKind::TransactionProgram,
        NodeKind::ActionSchema,
        NodeKind::ValidationShape,
        NodeKind::Correspondence,
        NodeKind::Annotation,
    ]
    .iter()
    .map(|k| k.as_str())
    .collect();

    let from_ttl = members(observation, "NodeKind");
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        from_rust, from_ttl_refs,
        "NodeKind enum must match the logic:NodeKind individuals in module.ttl"
    );

    // Round-trip through from_local, including the reserved ninth Correspondence slot.
    for k in &from_rust {
        assert_eq!(NodeKind::from_local(k).unwrap().as_str(), *k);
    }
    assert!(
        from_rust.contains("Correspondence"),
        "the reserved ninth kind is present"
    );
    assert_eq!(NodeKind::default(), NodeKind::ObjectLevelFormula);
}

fn formula_shape_values_match_module_ttl(observation: &Observation) {
    let from_rust: std::collections::BTreeSet<&str> =
        FormulaShape::ALL.iter().map(|s| s.as_str()).collect();

    let from_ttl = members(observation, "FormulaShape");
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        from_rust, from_ttl_refs,
        "FormulaShape enum must match the logic:FormulaShape individuals in module.ttl"
    );

    // as_str ↔ from_local round-trips for every variant; ALL is in canonical order.
    for s in FormulaShape::ALL {
        assert_eq!(FormulaShape::from_local(s.as_str()), Some(s));
    }
    assert!(FormulaShape::from_local("NotAShape").is_none());
    let ordered: Vec<&str> = FormulaShape::ALL.iter().map(|s| s.as_str()).collect();
    let mut sorted = ordered.clone();
    sorted.sort_unstable();
    assert_eq!(
        ordered, sorted,
        "ALL must be declared in as_str-lexical order"
    );
}

fn correspondence_relation_values_match_module_ttl(observation: &Observation) {
    let rust = [
        CorrespondenceRelation::Equiv,
        CorrespondenceRelation::Subsumes,
        CorrespondenceRelation::SubsumedBy,
        CorrespondenceRelation::Overlaps,
        CorrespondenceRelation::RelatedMatch,
        CorrespondenceRelation::Disjoint,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_members(observation, &names, "CorrespondenceRelation");
    for r in &rust {
        assert_eq!(CorrespondenceRelation::from_local(r.as_str()), Some(*r));
    }
}

fn morphism_class_values_match_module_ttl(observation: &Observation) {
    let rust = [
        MorphismClass::Isomorphism,
        MorphismClass::SectionRetraction,
        MorphismClass::WellBehavedLens,
        MorphismClass::LossyLens,
        MorphismClass::Prism,
        MorphismClass::AffineCorrespondence,
        MorphismClass::BridgeView,
    ];
    assert_eq!(rust.len(), 7, "the law-spine has seven rungs");
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_members(observation, &names, "MorphismClass");
    for r in &rust {
        assert_eq!(MorphismClass::from_local(r.as_str()), Some(*r));
    }
    // The derived Ord is the spine order (strongest first): Isomorphism is the top,
    // BridgeView the floor.
    assert!(MorphismClass::Isomorphism < MorphismClass::BridgeView);
    assert!(MorphismClass::Prism < MorphismClass::AffineCorrespondence);
}

fn morphism_kind_values_match_module_ttl(observation: &Observation) {
    let rust = [
        MorphismKind::InstitutionMorphism,
        MorphismKind::CommitmentShiftingBridge,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_members(observation, &names, "MorphismKind");
    for r in &rust {
        assert_eq!(MorphismKind::from_local(r.as_str()), Some(*r));
    }
}

fn determinacy_values_match_module_ttl(observation: &Observation) {
    let rust = [Determinacy::Crisp, Determinacy::Vague];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_members(observation, &names, "Determinacy");
    for r in &rust {
        assert_eq!(Determinacy::from_local(r.as_str()), Some(*r));
    }
}

fn correspondence_law_values_match_module_ttl(observation: &Observation) {
    let rust = [
        CorrespondenceLaw::GetPut,
        CorrespondenceLaw::PutGet,
        CorrespondenceLaw::PutPut,
        CorrespondenceLaw::SectionLaw,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_members(observation, &names, "CorrespondenceLaw");
    for r in &rust {
        assert_eq!(CorrespondenceLaw::from_local(r.as_str()), Some(*r));
    }
}

fn discharge_verdict_values_match_module_ttl(observation: &Observation) {
    // Reused from the foundation's non-entailment machinery; the IR enum mirrors it.
    let rust = [
        DischargeVerdict::ObligationDischarged,
        DischargeVerdict::ObligationUnknown,
        DischargeVerdict::ObligationViolated,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_members(observation, &names, "DischargeVerdict");
    for r in &rust {
        assert_eq!(DischargeVerdict::from_local(r.as_str()), Some(*r));
    }
}

fn discharge_condition_values_match_module_ttl(observation: &Observation) {
    let rust = [
        DischargeCondition::DischargeCertifiedFragment,
        DischargeCondition::DischargeFiniteClosure,
        DischargeCondition::DischargeSyntacticReachability,
        DischargeCondition::DischargeConservativeExtension,
        DischargeCondition::DischargeBoundedCorpus,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_members(observation, &names, "DischargeCondition");
    for r in &rust {
        assert_eq!(DischargeCondition::from_local(r.as_str()), Some(*r));
    }
}
