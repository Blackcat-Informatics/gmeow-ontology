// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from
//! slices/extensions/music/tests/test_music_performance.py (whole file; the
//! Python file is deleted).
//!
//! The 9 asserted-TBox / ABox guards all run over the music slice `module.ttl`
//! (`GraphStore::parse_ttl_file`): every performance-layer class, value
//! vocabulary, property axiom, and fixture — including the graphic-score
//! ScoreEdition/Expression derivation whose classes come from the core
//! creative-works slice but whose *fixture typing triples* are authored in the
//! music module — is present in the music module.ttl, so no merged ontology is
//! needed. `test_may_follow_is_not_dl_axiomatized` reproduces the Python ASK
//! (mayFollow carries neither `owl:TransitiveProperty` nor any
//! `owl:propertyChainAxiom` membership) via `GraphStore::ask`.
//!
//! The 11 SHACL guards build the inline instance graphs the Python assembled via
//! `g.add(...)` and validate them against the whole shapes corpus
//! (`Case::inline`), reproducing the honest in-graph ABox completion (P11): the
//! referenced MusicalParameter / DeterminationStatus / GenerativeProcessKind /
//! OrnamentProfileKind value individuals and the referenced TraversalConstraint
//! target are restated in-graph so the generated `sh:class` shapes resolve
//! without loading the merged ontology.
//!
//! source -> dest map:
//!   test_degree_of_freedom_classes_exist                   -> degree_of_freedom_classes_exist
//!   test_value_vocabularies_exist                          -> value_vocabularies_exist
//!   test_performance_functional_properties                 -> performance_functional_properties
//!   test_may_follow_is_not_dl_axiomatized                  -> may_follow_is_not_dl_axiomatized
//!   test_four_thirty_three_fixture_exists                  -> four_thirty_three_fixture_exists
//!   test_klavierstuck_xi_fixture_exists                    -> klavierstuck_xi_fixture_exists
//!   test_generative_process_fixture_exists                 -> generative_process_fixture_exists
//!   test_ornament_profile_fixture_exists                   -> ornament_profile_fixture_exists
//!   test_graphic_score_fixture_exists                      -> graphic_score_fixture_exists
//!   test_degree_of_freedom_valid_passes_shacl              -> case::degree_of_freedom_valid_passes
//!   test_degree_of_freedom_missing_parameter_fails_shacl   -> case::degree_of_freedom_missing_parameter_fails
//!   test_degree_of_freedom_both_targets_fails_shacl        -> case::degree_of_freedom_both_targets_fails
//!   test_traversal_constraint_valid_passes_shacl           -> case::traversal_constraint_valid_passes
//!   test_traversal_constraint_missing_text_fails_shacl     -> case::traversal_constraint_missing_text_fails
//!   test_performance_decision_valid_passes_shacl           -> case::performance_decision_valid_passes
//!   test_performance_decision_missing_sequence_fails_shacl -> case::performance_decision_missing_sequence_fails
//!   test_generative_process_valid_passes_shacl             -> case::generative_process_valid_passes
//!   test_generative_process_missing_rule_fails_shacl       -> case::generative_process_missing_rule_fails
//!   test_ornament_profile_valid_passes_shacl               -> case::ornament_profile_valid_passes
//!   test_ornament_profile_missing_target_fails_shacl       -> case::ornament_profile_missing_target_fails

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const MUSIC_MODULE: &str = "slices/extensions/music/module.ttl";

fn g(local: &str) -> String {
    format!("{G}{local}")
}
fn module() -> &'static GraphStore {
    static STORE: std::sync::OnceLock<GraphStore> = std::sync::OnceLock::new();
    STORE.get_or_init(|| GraphStore::parse_ttl_file(&repo_root().join(MUSIC_MODULE)))
}

// ── Asserted-TBox / ABox guards (slice module) ────────────────────────────────

#[test]
fn degree_of_freedom_classes_exist() {
    let s = module();
    for term in [
        "DegreeOfFreedom",
        "MusicalParameter",
        "DeterminationStatus",
        "TraversalConstraint",
        "PerformanceDecision",
        "GenerativeProcess",
        "GenerativeProcessKind",
        "OrnamentProfile",
        "OrnamentProfileKind",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(OWL_CLASS)),
            "{term} should be an owl:Class"
        );
    }
}

#[test]
fn value_vocabularies_exist() {
    let s = module();
    for term in [
        "musicalParameterPitch",
        "musicalParameterDuration",
        "musicalParameterOrder",
        "musicalParameterTempo",
        "musicalParameterDynamics",
        "musicalParameterTimbre",
        "musicalParameterInstrumentation",
        "musicalParameterPerformerCount",
        "musicalParameterSoundContent",
        "musicalParameterLocation",
        "musicalParameterTacet",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("MusicalParameter"))),
            "{term} should be a MusicalParameter"
        );
    }
    for term in [
        "determinationFixed",
        "determinationConstrained",
        "determinationFree",
        "determinationDelegatedPerformer",
        "determinationDelegatedEnvironment",
        "determinationDelegatedProcess",
    ] {
        assert!(
            s.has(
                Some(&g(term)),
                Some(RDF_TYPE),
                Some(&g("DeterminationStatus"))
            ),
            "{term} should be a DeterminationStatus"
        );
    }
    for term in [
        "generativeProcessKindPhasing",
        "generativeProcessKindStochastic",
        "generativeProcessKindVerbalScore",
        "generativeProcessKindRuleBased",
        "generativeProcessKindAlgorithmic",
    ] {
        assert!(
            s.has(
                Some(&g(term)),
                Some(RDF_TYPE),
                Some(&g("GenerativeProcessKind"))
            ),
            "{term} should be a GenerativeProcessKind"
        );
    }
    for term in [
        "ornamentProfileKindGamaka",
        "ornamentProfileKindBaroqueAgrement",
        "ornamentProfileKindJazzTurn",
        "ornamentProfileKindMordent",
        "ornamentProfileKindGraceNote",
    ] {
        assert!(
            s.has(
                Some(&g(term)),
                Some(RDF_TYPE),
                Some(&g("OrnamentProfileKind"))
            ),
            "{term} should be an OrnamentProfileKind"
        );
    }
}

#[test]
fn performance_functional_properties() {
    // Functionality is now carried by the canonical `logic:` characteristic records
    // (in the logic slice), not by a local `owl:FunctionalProperty` marker on the music
    // module — so this asserts over the merged ontology, not `module()`.
    let s = GraphStore::ontology();
    for prop in [
        "dofWork",
        "dofExpression",
        "dofParameter",
        "dofStatus",
        "constraintAppliesTo",
        "decisionPerformance",
        "decisionConstraint",
        "decisionSequence",
        "processKind",
        "ornamentProfileKind",
        "ornamentReferenceFrame",
    ] {
        assert!(
            s.is_functional_carrier(&g(prop)),
            "{prop} should carry a logic: functionalProperty characteristic"
        );
    }
    for prop in [
        "dofConstraintText",
        "dofConstraintFunction",
        "mayFollow",
        "constraintText",
        "constraintFunction",
        "processFunction",
        "processParameter",
        "processRuleText",
        "appliesToSegment",
        "appliesToVoice",
        "ornamentDescription",
    ] {
        assert!(
            !s.is_functional_carrier(&g(prop)),
            "{prop} should NOT carry a logic: functionalProperty characteristic"
        );
    }
}

#[test]
fn may_follow_is_not_dl_axiomatized() {
    let s = module();
    // No transitive declaration on mayFollow.
    assert!(!s.has(Some(&g("mayFollow")), Some(RDF_TYPE), Some(OWL_TRANSITIVE)));
    // No property chain axiom on mayFollow itself, and mayFollow does not appear
    // as a member of any property chain.
    let query = "
        PREFIX owl: <http://www.w3.org/2002/07/owl#>
        PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        ASK WHERE {
            {
                gmeow:mayFollow owl:propertyChainAxiom ?chain .
            } UNION {
                ?property owl:propertyChainAxiom ?chain .
                ?chain rdf:rest*/rdf:first gmeow:mayFollow .
            }
        }
    ";
    assert!(!s.ask(&[], query), "mayFollow must not be DL-axiomatized");
}

#[test]
fn four_thirty_three_fixture_exists() {
    let s = module();
    assert!(s.has(
        Some(&g("fixtureFourThirtyThreeWork")),
        Some(RDF_TYPE),
        Some(&g("MusicalWork"))
    ));
    for term in [
        "dofFourThirtyThreeDuration",
        "dofFourThirtyThreeTacet",
        "dofFourThirtyThreeSoundContent",
        "dofFourThirtyThreeInstrumentation",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("DegreeOfFreedom"))),
            "{term} should be a DegreeOfFreedom"
        );
    }
}

#[test]
fn klavierstuck_xi_fixture_exists() {
    let s = module();
    assert!(s.has(
        Some(&g("fixtureKlavierstuckXIWork")),
        Some(RDF_TYPE),
        Some(&g("MusicalWork"))
    ));
    assert!(s.has(
        Some(&g("fixtureKlavierstuckConstraint")),
        Some(RDF_TYPE),
        Some(&g("TraversalConstraint"))
    ));
    for term in [
        "fixtureKlavierstuckFragmentA",
        "fixtureKlavierstuckFragmentB",
        "fixtureKlavierstuckFragmentC",
        "fixtureKlavierstuckFragmentD",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("MusicalSegment"))),
            "{term} should be a MusicalSegment"
        );
    }
    for term in [
        "fixtureKlavierstuckDecisionOne",
        "fixtureKlavierstuckDecisionTwo",
    ] {
        assert!(
            s.has(
                Some(&g(term)),
                Some(RDF_TYPE),
                Some(&g("PerformanceDecision"))
            ),
            "{term} should be a PerformanceDecision"
        );
    }
}

#[test]
fn generative_process_fixture_exists() {
    let s = module();
    assert!(s.has(
        Some(&g("fixtureReichPhasingProcess")),
        Some(RDF_TYPE),
        Some(&g("GenerativeProcess"))
    ));
    assert!(s.has(
        Some(&g("fixtureReichPhasingProcess")),
        Some(&g("processFunction")),
        Some(&g("fnRealizePhasing"))
    ));
    assert!(s.has(
        Some(&g("fixtureXenakisStochasticProcess")),
        Some(RDF_TYPE),
        Some(&g("GenerativeProcess"))
    ));
}

#[test]
fn ornament_profile_fixture_exists() {
    let s = module();
    assert!(s.has(
        Some(&g("fixtureYamanVoice")),
        Some(RDF_TYPE),
        Some(&g("Voice"))
    ));
    assert!(s.has(
        Some(&g("fixtureYamanOrnamentProfile")),
        Some(RDF_TYPE),
        Some(&g("OrnamentProfile"))
    ));
    assert!(s.has(
        Some(&g("fixtureYamanOrnamentProfile")),
        Some(&g("ornamentProfileKind")),
        Some(&g("ornamentProfileKindGamaka"))
    ));
    assert!(s.has(
        Some(&g("fixtureYamanOrnamentProfile")),
        Some(&g("appliesToVoice")),
        Some(&g("fixtureYamanVoice"))
    ));
}

#[test]
fn graphic_score_fixture_exists() {
    let s = module();
    assert!(s.has(
        Some(&g("fixtureGraphicScoreWork")),
        Some(RDF_TYPE),
        Some(&g("MusicalWork"))
    ));
    assert!(s.has(
        Some(&g("fixtureGraphicScoreVisual")),
        Some(RDF_TYPE),
        Some(&g("ScoreEdition"))
    ));
    assert!(s.has(
        Some(&g("fixtureGraphicScoreTranscription")),
        Some(RDF_TYPE),
        Some(&g("Expression"))
    ));
    assert!(s.has(
        Some(&g("fixtureGraphicScoreTranscription")),
        Some(&g("wasDerivedFrom")),
        Some(&g("fixtureGraphicScoreVisual"))
    ));
}

// ── SHACL guards (whole shapes corpus, inline fixtures) ───────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test-music-performance/> .
";

#[rstest]
// A DegreeOfFreedom with a single Work target, a typed MusicalParameter, a typed
// DeterminationStatus, and a lang-tagged constraint text conforms.
#[case::degree_of_freedom_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:dofValid a gmeow:DegreeOfFreedom .
ex:dofValid gmeow:dofWork ex:work .
ex:dofValid gmeow:dofParameter gmeow:musicalParameterDuration .
ex:dofValid gmeow:dofStatus gmeow:determinationConstrained .
gmeow:musicalParameterDuration a gmeow:MusicalParameter .
gmeow:determinationConstrained a gmeow:DeterminationStatus .
ex:dofValid gmeow:dofConstraintText \"Total duration 4'33\\\".\"@x-gmeow-english .
"
)))]
#[case::degree_of_freedom_missing_parameter_fails(Case::inline(format!(
    "{PREFIXES}\
ex:dofBad a gmeow:DegreeOfFreedom .
ex:dofBad gmeow:dofWork ex:work .
ex:dofBad gmeow:dofStatus gmeow:determinationFree .
"
)).fails().violations(&["exactly one MusicalParameter"]))]
#[case::degree_of_freedom_both_targets_fails(Case::inline(format!(
    "{PREFIXES}\
ex:dofBad a gmeow:DegreeOfFreedom .
ex:dofBad gmeow:dofWork ex:work .
ex:dofBad gmeow:dofExpression ex:expression .
ex:dofBad gmeow:dofParameter gmeow:musicalParameterPitch .
ex:dofBad gmeow:dofStatus gmeow:determinationFree .
"
)).fails().violations(&["exactly one Work or exactly one Expression"]))]
#[case::traversal_constraint_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:constraintValid a gmeow:TraversalConstraint .
ex:constraintValid gmeow:constraintAppliesTo ex:work .
ex:constraintValid gmeow:constraintText \"Choose any fragment; stop after three repeats.\"@x-gmeow-english .
"
)))]
#[case::traversal_constraint_missing_text_fails(Case::inline(format!(
    "{PREFIXES}\
ex:constraintBad a gmeow:TraversalConstraint .
ex:constraintBad gmeow:constraintAppliesTo ex:work .
"
))
// `TraversalConstraint`'s at-least-one-rule-text bound is now PROJECTED SHACL derived
// from the EL-safe `logic:Restriction` axioms in `slices/extensions/music/module.ttl`
// (`generated/shapes/validation-shapes.ttl`, `gmeow:TraversalConstraint-shape`), which
// carries no `sh:message` (the prose-message convention was retired with the
// shapes-to-logic migration; see docs/MIGRATING-SHAPES-TO-LOGIC.md). `whole_shapes()`
// deliberately drops that file from this fixture corpus, so the projected bound is
// exercised against the live production shape union and asserted by path + constraint
// component — same rationale as `conformance_music_time`.
.shape_union()
.fails()
.fails_on_path(
    "https://blackcatinformatics.ca/gmeow/constraintText",
    "MinCountConstraintComponent",
))]
// A PerformanceDecision needs a performance Event, a constraint that itself
// satisfies TraversalConstraintShape (applies-to + rule text), and a sequence.
#[case::performance_decision_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:decisionValid a gmeow:PerformanceDecision .
ex:decisionValid gmeow:decisionPerformance ex:performance .
ex:decisionValid gmeow:decisionConstraint ex:constraint .
ex:decisionValid gmeow:decisionSequence \"A \u{2192} B \u{2192} C\" .
ex:performance a gmeow:Event .
ex:constraint a gmeow:TraversalConstraint .
ex:constraint gmeow:constraintAppliesTo ex:performance .
ex:constraint gmeow:constraintText \"Perform section A before section B.\"@x-gmeow-english .
"
)))]
#[case::performance_decision_missing_sequence_fails(Case::inline(format!(
    "{PREFIXES}\
ex:decisionBad a gmeow:PerformanceDecision .
ex:decisionBad gmeow:decisionPerformance ex:performance .
ex:decisionBad gmeow:decisionConstraint ex:constraint .
"
))
// `PerformanceDecision`'s exactly-one-sequence bound is the same kind of projected,
// message-less SHACL as the traversal rule-text bound above
// (`gmeow:PerformanceDecision-shape`) — same fix, same rationale.
.shape_union()
.fails()
.fails_on_path(
    "https://blackcatinformatics.ca/gmeow/decisionSequence",
    "MinCountConstraintComponent",
))]
#[case::generative_process_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:processValid a gmeow:GenerativeProcess .
ex:processValid gmeow:processKind gmeow:generativeProcessKindPhasing .
gmeow:generativeProcessKindPhasing a gmeow:GenerativeProcessKind .
ex:processValid gmeow:processRuleText \"Voice A and B begin in unison; B accelerates until one beat ahead.\"@x-gmeow-english .
"
)))]
#[case::generative_process_missing_rule_fails(Case::inline(format!(
    "{PREFIXES}\
ex:processBad a gmeow:GenerativeProcess .
ex:processBad gmeow:processKind gmeow:generativeProcessKindStochastic .
"
))
// `GenerativeProcess`'s at-least-one-rule-text bound is the same kind of projected,
// message-less SHACL as the two bounds above (`gmeow:GenerativeProcess-shape`,
// `sh:minCount 1` on `gmeow:processRuleText`) — same fix, same rationale.
.shape_union()
.fails()
.fails_on_path(
    "https://blackcatinformatics.ca/gmeow/processRuleText",
    "MinCountConstraintComponent",
))]
#[case::ornament_profile_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:ornamentValid a gmeow:OrnamentProfile .
ex:ornamentValid gmeow:ornamentProfileKind gmeow:ornamentProfileKindGamaka .
ex:ornamentValid gmeow:appliesToVoice ex:voice .
gmeow:ornamentProfileKindGamaka a gmeow:OrnamentProfileKind .
"
)))]
#[case::ornament_profile_missing_target_fails(Case::inline(format!(
    "{PREFIXES}\
ex:ornamentBad a gmeow:OrnamentProfile .
ex:ornamentBad gmeow:ornamentProfileKind gmeow:ornamentProfileKindGamaka .
"
)).fails().violations(&["at least one MusicalSegment or Voice"]))]
fn performance_shacl(#[case] case: Case) {
    case.run();
}
