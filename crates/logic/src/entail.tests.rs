// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const RDF_XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

fn dataset(nq: &str) -> Arc<RdfDataset> {
    purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
        .unwrap_or_else(|e| panic!("N-Quads parse failed: {e}\n{nq}"))
}

#[test]
fn refutation_domain_is_owned_by_the_admitted_default_reduction() {
    let premise = dataset(
        "<urn:entail:a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:entail:b> .\n\
             <urn:entail:x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:entail:a> .",
    );
    let admitted = AdmittedDefaultGraph::new(&premise, "premise").unwrap();
    let shape = ConclusionShape::GroundType {
        subject: "urn:entail:x".to_owned(),
        class: "urn:entail:b".to_owned(),
    };
    let mut iris = BTreeSet::new();
    collect_iris(&premise, &mut iris);
    let negation = negate(&shape, &Minter::new(&iris).unwrap()).unwrap();
    let result = reason_refutation(&admitted, &negation).unwrap();
    let native = result.native_execution().unwrap();
    assert_eq!(native.selected_domains.worlds().len(), 1);
    let selected = &native.selected_domains.worlds()[0];
    assert_eq!(selected.world().unwrap(), ENTAIL_WORLD);
    assert_eq!(selected.authority(), "gmeow.entail.default-refutation.v1");
    assert_eq!(
        native.class_admission.source_worlds[ENTAIL_WORLD].assertions, 4,
        "both original assertions and both reduction assertions retain their world",
    );
    let verdict = result.native_verdict().unwrap();
    assert!(!verdict.consistent);
    assert!(verdict.gaps.is_empty());
    assert!(
        verdict
            .inconsistencies
            .iter()
            .all(|witness| witness.world == ENTAIL_WORLD)
    );
}

/// Premise `a ⊑ b`, `x ∈ a` entails `x ∈ b`.
#[test]
fn ground_type_positive_entailment_is_entailed() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n\
             <http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n",
    );
    let conclusion = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/b> .\n",
    );
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::Entailed
    );
}

#[test]
fn entailment_assessment_retains_actual_component_and_inference_work() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
    );
    let conclusion = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
    );

    let assessment = dl_entailment_assessment(premise.as_ref(), conclusion.as_ref()).unwrap();
    assert_eq!(assessment.verdict, EntailmentVerdict::Entailed);
    assert_eq!(assessment.decisions, 1);
    assert!(
        assessment.steps > 0,
        "the asserted-type refutation must disclose its real native inference work"
    );
    assert_eq!(
        assessment.budget, None,
        "the installed operation declares no artificial finite ceiling"
    );
}

#[test]
fn empty_entailment_assessment_is_explicitly_zero_work() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
    );
    let conclusion = dataset("");

    let assessment = dl_entailment_assessment(premise.as_ref(), conclusion.as_ref()).unwrap();
    assert_eq!(assessment.verdict, EntailmentVerdict::Entailed);
    assert_eq!(assessment.decisions, 0);
    assert_eq!(assessment.steps, 0);
    assert_eq!(assessment.budget, None);
}

/// Premise `x ∈ a` alone does NOT entail `x ∈ b`.
#[test]
fn ground_type_non_entailment_is_not_entailed() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
    );
    let conclusion = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/b> .\n",
    );
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

/// Premise `a ⊑ b`, `b ⊑ c` entails `a ⊑ c` (subsumption, via a fresh witness).
#[test]
fn subclass_positive_entailment_is_entailed() {
    let premise = dataset(
        "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n\
             <http://ex/b> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
    );
    let conclusion = dataset(
        "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
    );
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::Entailed
    );
}

/// `a ⊑ b` does NOT entail `a ⊑ c`.
#[test]
fn subclass_non_entailment_is_not_entailed() {
    let premise = dataset(
        "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n",
    );
    let conclusion = dataset(
        "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
    );
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

/// A premise authored in the CANONICAL `logic:` subsumption vocabulary entails the
/// same memberships and subsumptions as one authored in its `rdfs:` projection.
///
/// This exercises the native source-role semantics through
/// [`crate::reason::prepare_reasoning_input`] and one [`crate::reason::reason_all`]
/// execution. Every `module.ttl` authors subsumption
/// as `logic:subClassOf` (Principle 17 — `rdfs:` is one of its lossy projections),
/// so without the EDB-boundary lowering a consumer asking "is this class a
/// `math:MathConformanceFailure`?" of the shipped bundle gets `not-entailed`: the
/// enforcement fires while the taxonomy stays dark.
#[test]
fn canonical_logic_subsumption_is_entailment_equivalent_to_its_rdfs_projection() {
    const LOGIC_SUBCLASS: &str = "https://blackcatinformatics.ca/logic/subClassOf";
    let premise = dataset(&format!(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n\
             <http://ex/a> <{LOGIC_SUBCLASS}> <http://ex/b> .\n\
             <http://ex/b> <{LOGIC_SUBCLASS}> <http://ex/c> .\n"
    ));
    let membership = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/c> .\n",
    );
    let subsumption = dataset(
        "<http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
    );
    assert_eq!(
        dl_entails(premise.as_ref(), membership.as_ref()).unwrap(),
        EntailmentVerdict::Entailed,
        "x ∈ c must follow from a canonically-spelled a ⊑ b ⊑ c"
    );
    assert_eq!(
        dl_entails(premise.as_ref(), subsumption.as_ref()).unwrap(),
        EntailmentVerdict::Entailed,
        "a ⊑ c must follow from a canonically-spelled a ⊑ b ⊑ c"
    );

    // The lowering ADDS the taxonomy; it does not make everything entailed.
    let unrelated = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/d> .\n",
    );
    assert_eq!(
        dl_entails(premise.as_ref(), unrelated.as_ref()).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

/// A class-expression body authored in the CANONICAL `logic:` restriction vocabulary
/// entails exactly what its `owl:` projection does.
///
/// This exercises the native source-role interpretation of class expressions.
/// Every slice authors a value
/// restriction as `[ a logic:Restriction ; logic:onProperty P ; logic:allValuesFrom D ]`;
/// the fixed RL rule that acts on it (`cls-avf`) names `owl:onProperty` /
/// `owl:allValuesFrom` by W3C specification. Without the EDB-boundary lowering such a
/// body reaches the derived SHACL surface and contributes NOTHING to the DL closure —
/// a mandatory-value axiom that enforces in validation and is invisible to
/// `gmeow entails`.
#[test]
fn canonical_logic_restriction_is_entailment_equivalent_to_its_owl_projection() {
    const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
    const OWL: &str = "http://www.w3.org/2002/07/owl#";
    const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    // `C ⊑ ∀p.D`, `v ∈ C`, `v p w`  ⊨  `w ∈ D`.
    let body = |ns: &str, subclass: &str| {
        dataset(&format!(
            "<http://ex/C> <{subclass}> <http://ex/r> .\n\
                 <http://ex/r> <{TYPE}> <{ns}Restriction> .\n\
                 <http://ex/r> <{ns}onProperty> <http://ex/p> .\n\
                 <http://ex/r> <{ns}allValuesFrom> <http://ex/D> .\n\
                 <http://ex/v> <{TYPE}> <http://ex/C> .\n\
                 <http://ex/v> <http://ex/p> <http://ex/w> .\n"
        ))
    };
    let canonical = body(LOGIC, &format!("{LOGIC}subClassOf"));
    let projected = body(OWL, RDFS_SUBCLASS);
    let conclusion = dataset(&format!("<http://ex/w> <{TYPE}> <http://ex/D> .\n"));

    assert_eq!(
        dl_entails(projected.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::Entailed,
        "control: the `owl:`-spelled restriction must be read"
    );
    assert_eq!(
        dl_entails(canonical.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::Entailed,
        "the CANONICAL `logic:` restriction must decide identically to its `owl:` \
             projection — an authored class-expression body is reasoner content, not \
             shape-surface-only content"
    );

    // The lowering ADDS the restriction; it does not make everything entailed.
    let unrelated = dataset(&format!("<http://ex/w> <{TYPE}> <http://ex/E> .\n"));
    assert_eq!(
        dl_entails(canonical.as_ref(), unrelated.as_ref()).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

/// A multi-triple conjunctive conclusion is entailed iff EVERY component is.
#[test]
fn multi_triple_conjunction_all_entailed_is_entailed() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n\
             <http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n\
             <http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/c> .\n",
    );
    let conclusion = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/b> .\n\
             <http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/c> .\n",
    );
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::Entailed
    );
}

/// A multi-triple conclusion with one un-entailed component is NOT entailed
/// (the conjunction, not a disjunction).
#[test]
fn multi_triple_conjunction_one_failing_is_not_entailed() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n\
             <http://ex/a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://ex/b> .\n",
    );
    let conclusion = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/b> .\n\
             <http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/c> .\n",
    );
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

/// An empty conclusion is trivially entailed.
#[test]
fn empty_conclusion_is_entailed() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
    );
    let conclusion = dataset("");
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::Entailed
    );
}

/// A blank-node conclusion subject is an existential-witness gap, not a verdict.
#[test]
fn blank_node_conclusion_is_existential_gap() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
    );
    let conclusion =
        dataset("_:b <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n");
    let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
    assert!(
        matches!(
            v,
            EntailmentVerdict::Gap(EntailmentGap {
                shape: GapShape::ExistentialWitness,
                ..
            })
        ),
        "{v:?}"
    );
}

/// A role/property-assertion conclusion is a role-assertion gap (role negation is
/// not EL-expressible).
#[test]
fn role_assertion_conclusion_is_role_gap() {
    let premise = dataset("<http://ex/a> <http://ex/knows> <http://ex/b> .\n");
    let conclusion = dataset("<http://ex/a> <http://ex/knows> <http://ex/b> .\n");
    let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
    assert!(
        matches!(
            v,
            EntailmentVerdict::Gap(EntailmentGap {
                shape: GapShape::RoleAssertion,
                ..
            })
        ),
        "{v:?}"
    );
}

/// A conclusion typing an individual to a literal is malformed.
#[test]
fn literal_class_conclusion_is_malformed_gap() {
    let premise = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
    );
    let conclusion = dataset(&format!(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \"oops\"^^<{RDF_XSD_STRING}> .\n"
    ));
    let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
    assert!(
        matches!(
            v,
            EntailmentVerdict::Gap(EntailmentGap {
                shape: GapShape::Malformed,
                ..
            })
        ),
        "{v:?}"
    );
}

/// SOUNDNESS FLOOR: a premise that already mentions a reserved-namespace IRI is
/// rejected — a minted complement could otherwise collide and flip the verdict.
#[test]
fn reserved_namespace_input_hard_fails() {
    let premise = dataset(&format!(
        "<{ENTAIL_RESERVED_NS}complement-deadbeefdeadbeef> \
             <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n"
    ));
    let conclusion = dataset(
        "<http://ex/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://ex/a> .\n",
    );
    let err = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap_err();
    assert!(
        err.message().contains("reserved entailment IRI"),
        "expected a reserved-namespace hard fail, got {err}"
    );
}

/// [`CapabilityGapShape::ontology_individual_local`] is the single naming
/// authority for the `gmeow:GapShape` individuals: every variant maps to a
/// distinct local name, and [`CapabilityGapShape::is_reasoner_fragment_gap`]
/// is true for exactly the first four (reasoner-fragment gaps), false only for
/// `VendoringMultiGoal` (a vendoring-format limit, not a reasoner gap).
#[test]
fn ontology_individual_locals_are_five_distinct_and_match_fragment_gap_flag() {
    let locals: BTreeSet<&'static str> = CapabilityGapShape::ALL
        .iter()
        .map(CapabilityGapShape::ontology_individual_local)
        .collect();
    assert_eq!(
        locals.len(),
        5,
        "all 5 CapabilityGapShape variants must map to distinct gmeow:GapShape locals"
    );
    for shape in &CapabilityGapShape::ALL[..4] {
        assert!(
            shape.is_reasoner_fragment_gap(),
            "{shape:?} must be a reasoner-fragment gap"
        );
    }
    assert!(
        !CapabilityGapShape::VendoringMultiGoal.is_reasoner_fragment_gap(),
        "VendoringMultiGoal is a vendoring-format limit, not a reasoner-fragment gap"
    );
}

/// Positive transitive subproperty: `P ⊑ Q`, `Q ⊑ R` entails `P ⊑ R` by
/// reflexive-transitive reachability (rdfs5).
#[test]
fn subproperty_transitive_positive_is_entailed() {
    let premise = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n\
             <http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/R> .\n"
    ));
    let conclusion = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/R> .\n"
    ));
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::Entailed
    );
}

/// Reflexive subproperty: `P ⊑ P` is always entailed (rdfs6), regardless of edges.
#[test]
fn subproperty_reflexive_is_entailed() {
    let premise = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
    ));
    let conclusion = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
    ));
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::Entailed
    );
}

/// `owl:equivalentProperty` is mutual subproperty: `P ≡ Q` entails BOTH `P ⊑ Q` and
/// `Q ⊑ P`.
#[test]
fn subproperty_equivalent_property_both_directions_are_entailed() {
    let premise = dataset(&format!(
        "<http://ex/P> <{OWL_EQUIVALENT_PROPERTY}> <http://ex/Q> .\n"
    ));
    let p_sub_q = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
    ));
    let q_sub_p = dataset(&format!(
        "<http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
    ));
    assert_eq!(
        dl_entails(premise.as_ref(), p_sub_q.as_ref()).unwrap(),
        EntailmentVerdict::Entailed
    );
    assert_eq!(
        dl_entails(premise.as_ref(), q_sub_p.as_ref()).unwrap(),
        EntailmentVerdict::Entailed
    );
}

/// Negative subproperty in a restricted (pure-hierarchy) premise: `P ⊑ Q` does NOT
/// entail the reverse `Q ⊑ P` — unreachable, so a sound `NotEntailed`.
#[test]
fn subproperty_unreachable_restricted_is_not_entailed() {
    let premise = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
    ));
    let conclusion = dataset(&format!(
        "<http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
    ));
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

/// Unrelated subproperty edges do not entail the conclusion: a restricted premise
/// with only `X ⊑ Y` does NOT entail `P ⊑ Q`.
#[test]
fn subproperty_unrelated_restricted_is_not_entailed() {
    let premise = dataset(&format!(
        "<http://ex/X> <{RDFS_SUBPROPERTYOF}> <http://ex/Y> .\n"
    ));
    let conclusion = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
    ));
    assert_eq!(
        dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap(),
        EntailmentVerdict::NotEntailed
    );
}

/// SOUNDNESS GATE: when the premise carries a property-relating construct beyond
/// `subPropertyOf`/`equivalentProperty` (here an `owl:propertyChainAxiom`), an
/// unreachable conclusion is an honest `native-coverage` GAP — NEVER a guessed
/// `NotEntailed`, because the chain axiom could derive further subproperty facts.
#[test]
fn subproperty_property_construct_makes_unreachable_a_gap() {
    const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
    let premise = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n\
             <http://ex/R> <{OWL_PROPERTY_CHAIN_AXIOM}> _:chain .\n"
    ));
    // `Q ⊑ P` is unreachable, but the chain axiom voids the restricted-vocabulary gate.
    let conclusion = dataset(&format!(
        "<http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
    ));
    let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
    assert!(
        matches!(
            v,
            EntailmentVerdict::Gap(EntailmentGap {
                shape: GapShape::NativeCoverage,
                ..
            })
        ),
        "{v:?}"
    );
}

/// The same soundness gate fires for `owl:inverseOf` (another derivation-capable
/// property construct): an unreachable subproperty conclusion is a GAP, not a verdict.
#[test]
fn subproperty_inverse_of_construct_makes_unreachable_a_gap() {
    const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
    let premise = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n\
             <http://ex/P> <{OWL_INVERSE_OF}> <http://ex/Pinv> .\n"
    ));
    let conclusion = dataset(&format!(
        "<http://ex/Q> <{RDFS_SUBPROPERTYOF}> <http://ex/P> .\n"
    ));
    let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
    assert!(
        matches!(
            v,
            EntailmentVerdict::Gap(EntailmentGap {
                shape: GapShape::NativeCoverage,
                ..
            })
        ),
        "{v:?}"
    );
}

/// `negate` HARD-FAILS on a subproperty shape: it is decided by reachability, never
/// refutation, so a caller that routes it to `negate` violates the contract and must
/// not receive a silent empty/garbage negation.
#[test]
fn negate_refuses_subproperty_shape() {
    let minter = Minter::new(&BTreeSet::new()).unwrap();
    let shape = ConclusionShape::SubPropertyOf {
        sub: "http://ex/P".to_string(),
        sup: "http://ex/Q".to_string(),
    };
    let err = negate(&shape, &minter).unwrap_err();
    assert!(
        err.message().contains("subproperty conclusion"),
        "expected a subproperty-invariant hard fail, got {err}"
    );
}

/// A subproperty conclusion with a literal superproperty is malformed (not a
/// reachability edge and not refutable).
#[test]
fn subproperty_literal_object_is_malformed_gap() {
    let premise = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> <http://ex/Q> .\n"
    ));
    let conclusion = dataset(&format!(
        "<http://ex/P> <{RDFS_SUBPROPERTYOF}> \"oops\"^^<{RDF_XSD_STRING}> .\n"
    ));
    let v = dl_entails(premise.as_ref(), conclusion.as_ref()).unwrap();
    assert!(
        matches!(
            v,
            EntailmentVerdict::Gap(EntailmentGap {
                shape: GapShape::Malformed,
                ..
            })
        ),
        "{v:?}"
    );
}

/// The minter is deterministic and content-addressed: same class → same symbol,
/// different classes → different symbols, and complement ≠ witness.
#[test]
fn minted_symbols_are_deterministic_and_distinct() {
    let minter = Minter::new(&BTreeSet::new()).unwrap();
    assert_eq!(
        minter.complement("http://ex/a"),
        minter.complement("http://ex/a")
    );
    assert_ne!(
        minter.complement("http://ex/a"),
        minter.complement("http://ex/b")
    );
    assert_ne!(
        minter.complement("http://ex/a"),
        minter.witness("http://ex/a")
    );
    assert!(
        minter
            .complement("http://ex/a")
            .starts_with(ENTAIL_RESERVED_NS)
    );
}
