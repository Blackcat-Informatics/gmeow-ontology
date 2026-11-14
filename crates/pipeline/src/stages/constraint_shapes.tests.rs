// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::stages::native_query;
use purrdf::shapes::engine::parse_shapes;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn authenticated_constraint_shapes() -> String {
    String::from_utf8(
        crate::fixture::authenticated_artifact(
            &repo_root(),
            "stage-export-constraint-shapes",
            CONSTRAINT_SHAPES_PATH,
        )
        .expect("authenticated constraint-shapes projection"),
    )
    .expect("constraint-shapes utf8")
}

#[test]
fn members_terminates_on_a_cyclic_list() {
    // A malformed list whose rdf:rest loops b0 -> b1 -> b0 must not hang the walk;
    // each node is visited once and the walk stops when the cycle closes.
    let mut first = BTreeMap::new();
    first.insert("b0".to_string(), "https://example.org/M0".to_string());
    first.insert("b1".to_string(), "https://example.org/M1".to_string());
    let mut rest = BTreeMap::new();
    rest.insert("b0".to_string(), Object::Blank("b1".to_string()));
    rest.insert("b1".to_string(), Object::Blank("b0".to_string()));
    let edges = ListEdges { first, rest };
    let members = edges.members(&Object::Blank("b0".to_string()));
    assert_eq!(
        members,
        vec![
            "https://example.org/M0".to_string(),
            "https://example.org/M1".to_string()
        ],
        "a cyclic list must terminate after visiting each node once"
    );
}

fn authored(turtle: &str) -> Dataset {
    let mut acc = DatasetAccumulator::new();
    acc.add_turtle(turtle.as_bytes(), None, "test")
        .expect("parse authored fixture");
    acc.freeze().expect("freeze authored fixture")
}

const IRREFLEXIVE_ASSERTION: &str = "\
    @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
    @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
    gmeow:rankedAbove a logic:ObjectProperty .\n\
    gmeow:rankedAboveIrreflexivity a logic:PropertyCharacteristicAssertion ;\n\
        logic:characterizes gmeow:rankedAbove ;\n\
        logic:characteristicSort logic:irreflexiveProperty .\n";

#[test]
fn irreflexivity_target_class_is_read_from_logic_domain() {
    // The canonical authoring spelling is logic:domain; a property that carries only it
    // must still yield its constraint shape, targeting that class.
    let ds = authored(&format!(
        "{IRREFLEXIVE_ASSERTION}gmeow:rankedAbove logic:domain gmeow:Frame .\n"
    ));
    let ttl = render_from_dataset(&ds).expect("a logic:domain-only property projects");
    assert!(
        ttl.contains("sh:targetClass <https://blackcatinformatics.ca/gmeow/Frame>"),
        "{ttl}"
    );
    assert!(
        ttl.contains("RankedAboveIrreflexivityConstraintShape"),
        "{ttl}"
    );
}

#[test]
fn irreflexivity_target_class_still_reads_the_older_rdfs_domain() {
    let ds = authored(&format!(
        "{IRREFLEXIVE_ASSERTION}gmeow:rankedAbove rdfs:domain gmeow:Frame .\n"
    ));
    let ttl = render_from_dataset(&ds).expect("an rdfs:domain-only property projects");
    assert!(
        ttl.contains("sh:targetClass <https://blackcatinformatics.ca/gmeow/Frame>"),
        "{ttl}"
    );
}

#[test]
fn differing_domain_spellings_are_refused_naming_both_classes() {
    let ds = authored(&format!(
        "{IRREFLEXIVE_ASSERTION}gmeow:rankedAbove logic:domain gmeow:Frame ; rdfs:domain gmeow:Other .\n"
    ));
    let error = render_from_dataset(&ds).expect_err("contradictory domains must not project");
    let text = error.to_string();
    assert!(
        text.contains("gmeow/Frame") && text.contains("gmeow/Other"),
        "{text}"
    );
}

#[test]
fn a_property_with_no_domain_is_refused_naming_logic_domain() {
    let ds = authored(IRREFLEXIVE_ASSERTION);
    let error = render_from_dataset(&ds).expect_err("a domainless property must not project");
    assert!(error.to_string().contains("author logic:domain"), "{error}");
}

#[test]
fn all_axioms_project() {
    let ttl = authenticated_constraint_shapes();
    // 7 irreflexivity + 1 acyclicity + 7 distinctness + 4 disjointness + 3 conditional-range +
    // 1 role-composition-exclusion + 1 mediated-property-requirement = 24 shapes. Grounding the
    // inference + inhabitation proving slices added the attack/support self-exclusion distinctness,
    // the three kind→target conditional-range agreements, the argument-component exclusion, and the
    // inhabitation-interval frame requirement; grounding math/lang add Frege object-vs-reference and
    // linguistic act-vs-observation disjointness; the preference slice adds the three
    // irreflexivity characteristics of its cell-order relations (gmeow:strictlyOver,
    // gmeow:preferentiallyEquivalentWith, gmeow:incomparableWith) — nothing is strictly preferred
    // to, tied with, or incomparable to itself under any vantage; the standpoint slice adds the
    // irreflexivity of gmeow:moreEntrenchedThan — no standpoint is ranked above itself.
    assert_eq!(
        ttl.matches("a sh:NodeShape").count(),
        24,
        "exactly twenty-four FOL axioms must project to constraint shapes"
    );
    for anchor in [
        "gmeow:counterGoal",
        "gmeow:overrides",
        "gmeow:linkNext",
        "committedAgent",
        "identityAxisDisjointness",
        "softwareFacetDisjointness",
        "competesWithIrreflexivity",
        "inferencePremiseConclusionDistinctness",
        "attackSelfAttackExclusion",
        "supportSelfSupportExclusion",
        "attackUndermineTargetsPremiseUse",
        "attackUndercutTargetsInferenceApplication",
        "attackRebutTargetsStandpointClaim",
        "attackComponentSelfExclusion",
        "inhabitationIntervalFrameRequirement",
        "ActObservationDisjointness",
        "FregeDisjointness",
        "StrictlyOverIrreflexivity",
        "PreferentiallyEquivalentWithIrreflexivity",
        "IncomparableWithIrreflexivity",
        "MoreEntrenchedThanIrreflexivity",
    ] {
        assert!(ttl.contains(anchor), "expected {anchor} in the projection");
    }
    // Every constraint block must carry a logic:formalizes back-reference triple
    // (the header comment mentions the term in prose, so match the triple form).
    assert_eq!(
        ttl.matches("logic:formalizes <").count(),
        24,
        "every projected shape must carry a logic:formalizes back-reference"
    );
}

#[test]
fn projection_flags_each_family_and_passes_clean_data() {
    // Prove the projection is NOT vacuous: the REAL generated constraint-shapes must
    // flag a planted violation of each family (irreflexivity, distinctness, acyclicity,
    // disjointness) and pass a clean control — the equivalence-before-deletion evidence
    // that the migrated axioms keep their SHACL teeth.
    use purrdf::shapes::engine::{parse_shapes, validate_dataset};

    let ttl = authenticated_constraint_shapes();
    let shapes = parse_shapes(&ttl, None).expect("parse generated constraint-shapes");

    let data = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:selfGoal a gmeow:Goal ; gmeow:counterGoal ex:selfGoal .\n\
            ex:okGoal   a gmeow:Goal ; gmeow:counterGoal ex:otherGoal .\n\
            ex:selfCommit a gmeow:Commitment ; gmeow:committedAgent ex:a ; gmeow:commitmentBeneficiary ex:a .\n\
            ex:okCommit   a gmeow:Commitment ; gmeow:committedAgent ex:a ; gmeow:commitmentBeneficiary ex:b .\n\
            ex:cycleLink a gmeow:CausalLink ; gmeow:linkNext ex:cycleLink .\n\
            ex:overtyped a gmeow:GenderIdentity, gmeow:GenderExpression .\n\
            ex:badKindAttack a gmeow:Attack ; gmeow:attackKind gmeow:attackUndermine ; gmeow:attackTarget ex:someClaim ; gmeow:attackSource ex:argX .\n\
            ex:someClaim a gmeow:StandpointClaim .\n\
            ex:argX a gmeow:Argument .\n\
            ex:okAttack a gmeow:Attack ; gmeow:attackKind gmeow:attackUndermine ; gmeow:attackTarget ex:premUse ; gmeow:attackSource ex:argY .\n\
            ex:premUse a gmeow:PremiseUse .\n\
            ex:argY a gmeow:Argument .\n\
            ex:selfCompAttack a gmeow:Attack ; gmeow:attackKind gmeow:attackRebut ; gmeow:attackTarget ex:conclClaim ; gmeow:attackSource ex:argZ .\n\
            ex:argZ a gmeow:Argument ; gmeow:argumentConclusion ex:conclClaim .\n\
            ex:conclClaim a gmeow:StandpointClaim .\n\
            ex:badTenure a gmeow:InhabitationTenure ; gmeow:duringInterval ex:noFrameInt .\n\
            ex:noFrameInt a gmeow:TimeInterval .\n\
            ex:okTenure a gmeow:InhabitationTenure ; gmeow:duringInterval ex:framedInt .\n\
            ex:framedInt a gmeow:TimeInterval ; gmeow:hasTemporalFrame ex:someFrame .\n";
    let store = native_query::dataset_from_turtle(data.as_bytes(), "test").unwrap();
    let report = validate_dataset(&store, &shapes).unwrap();
    let flagged: Vec<String> = report
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect();

    for bad in [
        "selfGoal",
        "selfCommit",
        "cycleLink",
        "overtyped",
        "badKindAttack",
        "selfCompAttack",
        "badTenure",
    ] {
        assert!(
            flagged.iter().any(|f| f.contains(bad)),
            "the {bad} violation must be flagged; flagged: {flagged:?}"
        );
    }
    for good in ["okGoal", "okCommit", "okAttack", "okTenure"] {
        assert!(
            !flagged.iter().any(|f| f.contains(good)),
            "the clean {good} node must NOT be flagged; flagged: {flagged:?}"
        );
    }
}

#[test]
fn generated_shapes_parse_as_a_shape_union_member() {
    // The generated document must parse in the shape-union loader (the SHACL lane
    // that consumes generated/shapes/*.ttl), proving it is well-formed SHACL Turtle.
    let ttl = authenticated_constraint_shapes();
    parse_shapes(&ttl, None).expect("generated constraint-shapes must parse as SHACL");
}
