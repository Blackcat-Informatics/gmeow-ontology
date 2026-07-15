// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Full slice-example validation sweep (Task 6).
//!
//! This integration test proves the closed-world fidelity of the SHACL→JSON
//! Schema projection over the WHOLE example corpus: for every `slices/*/*/
//! examples/*.ttl` data graph, the projected JSON-LD `@graph` instance form
//! validates against the JSON Schema the emitter derives from the SAME merged
//! shapes the live validator uses ([`purrdf::shapes::shape_union::load_shapes`]).
//!
//! # Soundness contract
//!
//! The JSON Schema is a CLOSED-WORLD projection of the SHACL shapes: it claims
//! to accept exactly the data the SHACL validator accepts (for the modeled
//! subset). Therefore:
//!
//! * If an example does NOT conform to its SHACL shapes, it is illustrative
//!   (not valid instance data) and OUT OF SCOPE for the schema sweep. Such
//!   examples are listed in [`NON_CONFORMANT`] with a one-line reason. The test
//!   asserts the excluded set is EXACTLY the set that fails native SHACL — so an
//!   exclusion can never silently mask a real schema bug.
//! * If an example DOES conform to SHACL but the JSON Schema REJECTS it, that is
//!   a soundness bug in the emitter/projector, surfaced as a test failure with a
//!   readable per-example violation report.

use std::path::{Path, PathBuf};

use std::sync::Arc;

use gmeow_validate::instance::{InstanceFormat, validate_instance};
use purrdf::shapes::shapes::Shapes;
use purrdf::shapes::{engine, instance, json_schema, shape_union};

use purrdf::RdfDataset;
use purrdf::parse_dataset;

/// Examples that do NOT conform to the merged SHACL shapes and are therefore
/// out of scope for the JSON-schema sweep (illustrative, not valid instance
/// data). The sweep asserts this set is EXACTLY the SHACL-failing set, so this
/// allowlist cannot hide a JSON-schema soundness bug.
///
/// Each entry is the repo-relative path; the trailing comment is the reason.
const NON_CONFORMANT: &[&str] = &[
    // Bucket A — `sh:class` (ClassConstraintComponent): the example references a
    // SHARED ontology individual (a method/status/kind/profile defined in the
    // vocabulary, not redeclared in the standalone fixture), so the referenced
    // node lacks its `rdf:type` when the file is loaded in isolation. The example
    // is meant to be read alongside the full ontology; standalone it is
    // illustrative, not valid instance data.
    "slices/core/ai/examples/grounded-claim.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/attestation/examples/release-evidence-bundle.ttl", // gmeow:attestedSubject/attester → Entity/Agent not typed standalone (CreativeWork/SoftwareAgent); gmeow:hasVerificationStatus → shared VerificationStatus individual untyped standalone
    "slices/core/attestation/examples/software-release.ttl", // gmeow:attestedSubject/attester → Entity/Agent not typed standalone; gmeow:hasVerificationStatus → shared VerificationStatus individual untyped standalone
    "slices/core/calendar/examples/recurring-meeting.ttl", // gmeow:invitationStatus → shared status individual untyped standalone
    "slices/core/citations/examples/citation-act.ttl", // gmeow:citationIntent → shared CitationIntent individual untyped standalone; gmeow:citingEntity → Entity not typed standalone
    "slices/core/cognition/examples/attention-interest-memory.ttl", // gmeow:memoryOf → Agent not typed standalone
    "slices/core/cognition/examples/dunning-kruger.ttl", // gmeow:knowledgeProficiencyAgent/Subject → Agent/Entity not typed standalone; gmeow:knowledgeProficiencyLevel/Scale → shared KnowledgeLevel/ProficiencyScale individuals untyped standalone
    "slices/core/cognition/examples/knowledge-proficiency.ttl", // gmeow:knowledgeProficiencyAgent/Subject → Agent/Entity not typed standalone; gmeow:knowledgeProficiencyLevel/Scale → shared KnowledgeLevel/ProficiencyScale individuals untyped standalone
    "slices/core/deception/examples/blame-deflection.ttl", // gmeow:doxasticClaim → StandpointClaim not typed standalone
    "slices/core/diagnostics/examples/shacl-violation-finding.ttl", // gmeow:findingSeverity → shared DiagnosticSeverity untyped standalone
    "slices/core/documentation/examples/documented-term.ttl", // gmeow:docEvidenceKind → shared gmeow:DocEvidenceKind individual (docEvidenceKindCompetency/Provenance) typed in module.ttl, untyped standalone
    "slices/core/epistemics/examples/belief-revision.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/epistemics/examples/claim-token-split.ttl", // gmeow:observationMethod → shared method individual (methodExpertJudgement) untyped standalone
    "slices/core/epistemics/examples/flagship-epistemic-ledger.ttl", // gmeow:epistemicAgent → Agent not typed standalone
    "slices/core/epistemics/examples/justification-and-defeat.ttl", // gmeow:hasDefeatStatus / supportUnderStandard → shared status/standard individuals untyped standalone
    "slices/core/epistemics/examples/locally-factive-knowledge.ttl", // gmeow:underStandard → gmeow:standardScientific (shared EpistemicStandard) + gmeow:knowerAgent → Agent untyped standalone
    "slices/core/events/examples/wedding.ttl", // gmeow:participationParticipant → Entity not typed standalone (the principals/officiant are gmeow:Person, standalone lacks the subClassOf→Entity chain)
    "slices/core/evidence/examples/notability-assessment.ttl", // gmeow:citationIntent → shared CitationIntent individual untyped standalone; gmeow:citingEntity → Entity not typed standalone
    "slices/core/expertise/examples/skill-proficiency.ttl", // gmeow:attestedSubject/attester/skillProficiencyAgent → Entity/Agent not typed standalone
    "slices/core/gender/examples/self-asserted-facets.ttl", // gmeow:expressionValue/genderValue → shared GenderExpressionStyle/Gender individuals untyped standalone
    "slices/core/gts/examples/dist-package.ttl", // gmeow:gtsProfile → shared profile individual untyped standalone
    "slices/core/imagination/examples/reality-monitoring.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/abduction.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/analogy.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/argumentation.ttl", // gmeow:observationMethod + gmeow:underSemantics → logic:GroundedArgumentation (shared) untyped standalone
    "slices/core/inference/examples/belief-revision.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/deduction.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/induction.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inhabitation/examples/continuity-upgrade.ttl", // gmeow:observationMethod (methodExpertJudgement), gmeow:continuityVerdict (continuitySame/Different), gmeow:determinationForce (forceBinding) → shared method/value individuals untyped standalone; gmeow:stageBearer/observedFeature → Agent/Entity not typed standalone (SoftwareAgent⊑Agent chain); gmeow:hasTemporalFrame → shared TemporalFrame untyped standalone
    "slices/core/inhabitation/examples/control.ttl", // gmeow:controlLevel (controlFull/controlPartial), gmeow:observationMethod (methodExpertJudgement) → shared value/method individuals untyped standalone; gmeow:controlOver → Entity not typed standalone (PhysicalObject⊑Entity chain); gmeow:hasTemporalFrame → shared TemporalFrame untyped standalone
    "slices/core/inhabitation/examples/inhabitation-tenure.ttl", // gmeow:inhabitationLocusKind (locusVessel), gmeow:eventType (eventTypeInhabitationTransition) → shared value/type individuals untyped standalone; gmeow:inhabitationSubject/inhabitedHost/assignmentSubject → Agent/Entity not typed standalone (SoftwareAgent⊑Agent, PhysicalObject⊑Entity chains); gmeow:hasTemporalFrame → shared TemporalFrame untyped standalone
    "slices/core/inhabitation/examples/subject-status.ttl", // gmeow:tenureSubjectAgent/tenureVantage → Agent not typed standalone (SoftwareAgent⊑Agent chain); gmeow:hasTemporalFrame → shared TemporalFrame untyped standalone
    "slices/extensions/model-serving/examples/tool-usage.ttl", // the CQ5 discriminator fixture uses logic:ActionSchema (ex:sortSchema) illustratively as the passive-capability TARGET of gmeow:usedCapability; standalone it carries none of the required logic:precondition / logic:capability facets (logic:ActionSchemaShape), so it is illustrative-alongside-the-ontology, not standalone-valid instance data
    "slices/core/inquiry/examples/loaded-question.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inquiry/examples/open-question-and-resolution.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/grounding/lang/examples/forms-and-sign-systems.ttl", // lang:partOfSpeech/slotRole/featureKey/featureValue/analysisLevel/compositionLevel/offsetSpace/signSystemKind/modality/grammarFormalism → shared inventory, role, level, and kind individuals (noun, subjectRole, featNumber, valPlur, parsedLevel, sentenceLevel, codepointOffset, naturalLanguageKind, writtenModality, ebnfFormalism) defined in module.ttl, untyped standalone
    "slices/grounding/lang/examples/gmn-dialect.ttl", // lang:denotationKind/gmeow:gmnSecurityRing/gmeow:citationIntent → shared kind, ring, and intent individuals (denotesEntity, gmnRingTrusted, intentConformsTo) defined in module.ttl files, untyped standalone; gmeow:vantage/accordingTo/citingEntity sh:class Entity → the Agent/InformationObject-typed nodes lack the subClassOf→Entity chain standalone
    "slices/grounding/lang/examples/flagship-acceptance.ttl", // lang:FlagshipScenarioShape's sh:sparql subclass check (?fc rdfs:subClassOf lang:LangConformanceFailure) needs module.ttl's failure-class subclass axioms, which are absent when the example is validated in closed-world isolation — so every scenario appears to name a non-failure class standalone; in the merged bundle it conforms (make validate passes)
    "slices/grounding/logic/examples/formalization-governance.ttl", // logic:candidateCategory/candidateProjectionBehavior/candidateNonEntailment → shared governance individuals (categories, preservation kinds, the standing obligations) defined in module.ttl, untyped standalone
    "slices/grounding/logic/examples/conjectures.ttl", // logic:candidateCategory/candidateProjectionBehavior/conjectureLifecycleState/conjectureDischargeVerdict → shared governance and Belnap-lifecycle value individuals defined in module.ttl, untyped standalone; the bare logic:Formula/GoalExpression/ContradictionWitness AST leaves (the denotation seam) have no closed-world schema entry standalone
    "slices/grounding/math/examples/measure-and-dimension.ttl", // math:exponentOfDimension → the shared SI base-dimension individuals (massDimension/lengthDimension/timeDimension) defined in module.ttl, untyped standalone; math:withRespectTo/hasDimension sh:class Measure/Dimension → the subclass-typed measure and dimension nodes (LebesgueMeasure/DerivedDimension) lack the subClassOf chain standalone
    "slices/grounding/math/examples/numbers-sets-functions.ttl", // math:hasElement → set-member individuals (two/three/five/seven) untyped standalone; math:memberCondition → a logic:Formula node (no closed-world schema entry, the denotation seam)
    "slices/grounding/math/examples/homomorphic-encryption.ttl", // math:encryptOperation/evaluateOperation/decryptOperation → gmeow:Activity process individuals, and the preservation law → a logic:Formula AST (the denotation seam), with no closed-world schema entry standalone
    "slices/grounding/math/examples/analysis-and-geometry.ttl", // math:operator/manifoldStructureKind/complementSemantics/convergenceMode/limitMode → shared binder/structure-kind/semantics/mode individuals (differentiationBinder/lorentzianStructure/setTheoreticComplement/absoluteConvergence/limitTwoSided) defined in module.ttl, untyped standalone; the bare math:MathematicalObject/MathematicalExpression AST leaves (worldlineExpr, originPoint, seriesLimit) have no closed-world schema entry
    "slices/grounding/math/examples/linear-algebra-and-learning.ttl", // math:complementSemantics/centeringPolicy/scalingPolicy/operator/tensorOperation → shared semantics/policy/operator individuals (orthogonalComplement/meanCentered/unitVariance/matrixProduct) defined in module.ttl, untyped standalone; the bare math:MathematicalExpression AST leaves (inputActivationExpr/layer1WeightExpr) and the logic:MetaLevelFormula reflection target (latentThemeFormula, the denotation seam) have no closed-world schema entry standalone
    "slices/grounding/math/examples/closed-form-functions.ttl", // the expression-algebra AST leaves (math:ApplicationExpression/NumberLiteral/VariableExpression) and the shared arithmetic operator individuals (math:Addition/Multiplication) defined in module.ttl are untyped/schema-less standalone; math:ClosedFormFunction's math:domain/codomain sh:class math:Set resolves only through the module subclass chain (math:realNumbers is a math:Set under the module axioms) — illustrative, validated unioned with the module by make validate
    "slices/grounding/math/examples/signed-radial-field-qualitative.ttl", // shared value individuals (math:openEndpoint/closedEndpoint, math:PositiveInfinity/NegativeInfinity, math:divergesToPositiveInfinity/divergesToNegativeInfinity, math:strictlyDecreasing, math:nonAffinity, math:boundedness, math:lorentzianStructure) defined in module.ttl are untyped standalone; math:domain/codomain and the math:Interval/MeasurableSet ring bands' sh:class math:Set resolve through the module subclass tower (math:Interval/math:MeasurableSet ⊑ math:Set only under the module axioms) — illustrative, validated unioned with the module by make validate
    "slices/grounding/math/examples/signed-radial-field-closed-form.ttl", // same shared value individuals as the qualitative scene, plus the closed-form defining-expression AST leaves (math:ApplicationExpression/NumberLiteral/VariableExpression) and shared arithmetic operators (math:Addition/Subtraction/Multiplication/Exponentiation/Negation) with no closed-world schema entry standalone; math:ClosedFormFunction domain/codomain and the ring bands' sh:class math:Set resolve through the module subclass chain — illustrative, validated unioned with the module by make validate
    "slices/grounding/math/examples/bridges.ttl", // math:ingestCorrespondence → logic:Correspondence and logic:instantiatesSchema/instantiatesPlan → logic:ActionSchema/Plan process-witness nodes, math:provesGoal → a logic:GoalExpression (with logic:goalExpressionKind/boundSituationType → logic:Situation), and math:compilesToLogicFormula → a logic:Formula (the denotation seam) — cross-slice logic: nodes with no closed-world math schema entry standalone; the gmeow:Observation/Standpoint claim nodes and the bare math:MathematicalObject source-witness/result AST leaves (rSrcWitness/onnxSource/proofSource, rFitSummary) likewise have no closed-world schema entry standalone
    "slices/grounding/math/examples/theorem-proof-claim.ttl", // math:statementRole/roleInTheory/verificationResult → shared value individuals (roleTheorem, verificationPassed) defined in module.ttl, untyped standalone; the gmeow:Observation/Standpoint held-claim nodes and the bare math:MathematicalObject/Axiom statement/theory/conclusion leaves have no closed-world schema entry standalone
    "slices/grounding/math/examples/flagship-acceptance.ttl", // math:FlagshipScenarioShape's sh:sparql subclass check (?fc rdfs:subClassOf math:MathConformanceFailure) needs module.ttl's failure-class subclass axioms, which are absent when the example is validated in closed-world isolation — so every scenario appears to name a non-failure class standalone; in the merged bundle it conforms (make validate passes)
    "slices/grounding/math/examples/probability.ttl", // gmeow:vantage → an Agent (weatherModelV17) untyped standalone; gmeow:hasReferenceFrame → reference frames whose full gmeow:FrameProfile (frameKind/frameRealm/hasAxis/dimensionCount/determinacyModel/requiresHost) and the gmeow:ScalarQuantity gmeow:unit of the probability/parameter values are carried against the module, not standalone; the math:ProbabilitySpace component qualified-cardinality (sampleSpace/eventSigmaAlgebra onClass math:SampleSpace/math:SigmaAlgebra) resolves through the module subclass tower (a math:SymbolicSampleSpace/math:BorelSigmaAlgebra is a math:SampleSpace/math:SigmaAlgebra only under the module axioms) — illustrative, validated unioned with the module by make validate
    "slices/grounding/math/examples/statistics-hypotheses-pvalues.ttl", // math:alternativeSidedness carries the six shared math:Sidedness NamedIndividuals (math:twoSidedAlternative/oneSidedAlternative/greaterAlternative/lessAlternative/exactTail/midPTail) defined in module.ttl — the closed-world JSON-Schema projection now enumerates that sh:in as {"@id": …} objects matching the instance projector, so those value nodes are not the isolation trigger; the actual SHACL-isolation trigger is its bare gmeow:ReferenceFrame (testFrame), whose full gmeow:FrameProfile (frameKind/frameRealm/hasAxis/dimensionCount/determinacyModel/requiresHost) is carried against the module, exactly like the sibling pvalue fixtures — illustrative, validated unioned with the module by make validate
    "slices/grounding/math/examples/pvalue-tri-slice.ttl", // the lang: -> logic: -> math: round-trip references shared kind/role/level/value individuals defined across the three modules (lang:naturalLanguageKind/writtenModality/sentenceLevel/subjectRole/predicateRole/objectRole/parsedLevel/denotesLogicFormula/assertForce, math:twoSidedAlternative) untyped standalone; the denotation-seam logic:Formula/logic:Type AST leaves (pvalueFormula, pValueMagnitudeOfRelation) and the logic:ExactPreservation preservation-kind individual have no closed-world schema entry standalone; the cross-slice lang:/logic: nodes have no math closed-world schema entry standalone — illustrative, validated unioned with the module by make validate (conformance twin tests/conformance-fixtures/pvalue-tri-slice.ttl conforms in the merged bundle)
    "slices/grounding/logic/examples/flagship-acceptance.ttl", // logic:FlagshipScenarioShape's sh:sparql subclass check (?fc rdfs:subClassOf logic:LogicConformanceFailure) needs module.ttl's failure-class subclass axioms, which are absent when the example is validated in closed-world isolation — so every scenario appears to name a non-failure class standalone; in the merged bundle it conforms (make validate passes)
    "slices/core/metacognition/examples/dunning-kruger.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/metacognition/examples/reflection-revision.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/names/examples/person-names.ttl", // gmeow:usageAppellation/usageNamed → Appellation/Entity not typed standalone
    "slices/core/observations/examples/blood-pressure.ttl", // gmeow:observationMethod (methodInstrumentalReading) + the reference frame's shared component individuals (determinacyCrisp, frameKindScalar, frameRealmMeasurement, axisScalar) untyped standalone
    "slices/core/observations/examples/temperature-reading.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/pipeline/examples/minimal-pipeline.ttl", // gmeow:hasCapability → shared gmeow:StageCapability untyped standalone
    "slices/core/places/examples/located-place.ttl", // gmeow:vantage → Agent not typed standalone (the survey team is gmeow:Organization)
    "slices/core/profiles/examples/named-profile-membership.ttl", // gmeow:profileAppliesTo → owl:Class target not typed standalone
    "slices/core/quality/examples/dataset-completeness.ttl", // gmeow:assessedEntity → Entity not typed standalone (the dataset is gmeow:Dataset)
    "slices/core/slice-quality-rubric/examples/rubric-assessment.ttl", // reuses the quality Observation stack exactly like dataset-completeness.ttl: gmeow:assessedEntity → Dataset not typed standalone, gmeow:observationMethod / gmeow:qualityDimension → shared value individuals defined in module.ttl, untyped standalone
    "slices/core/rights/examples/licensed-dataset.ttl", // gmeow:licensedWork/copyrightWork/statementAbout → InformationObject/Entity not typed standalone; gmeow:licensor/copyrightHolder → Agent not typed standalone
    "slices/core/sexuality/examples/split-attraction.ttl", // gmeow:romanticOrientationValue/sexualOrientationValue → shared RomanticOrientationValue/SexualOrientationValue individuals untyped standalone
    "slices/core/standpoint/examples/contested-authorship.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/tags/examples/contested-tagging.ttl", // gmeow:taggingTagged/taggingTagger → Entity/Agent not typed standalone; gmeow:hasTemporalFrame → shared temporalFrameUTCGregorian individual untyped standalone
    "slices/core/tags/examples/folksonomy.ttl", // gmeow:taggingTagged/taggingTagger → Entity/Agent not typed standalone
    "slices/core/trust/examples/web-of-trust.ttl", // gmeow:certifier/certifiedIdentity/trustor/trustee → Agent not typed standalone
    "slices/core/versions/examples/release-channels.ttl", // gmeow:membershipAuthority/versionMember → Agent/Entity not typed standalone
    "slices/extensions/accessibility/examples/location-access.ttl", // gmeow:assertionFacet/assertionPolarity → shared AccessibilityFacet/AccessibilityPolarity individuals untyped standalone; gmeow:assertionSubject → Entity not typed standalone
    "slices/extensions/aggregation/examples/spatial-bins.ttl", // gmeow:aggregationFunction → shared function individual untyped standalone
    "slices/extensions/archaeological-evidence/examples/inscription-reading.ttl", // gmeow:vantage → Entity not typed standalone (the epigraphers are gmeow:Person)
    "slices/extensions/dreaming/examples/ai-offline-replay.ttl", // gmeow:gtsProfile → shared profile individual untyped standalone
    "slices/extensions/dreaming/examples/lucid-dream.ttl", // gmeow:vantage → Entity not typed standalone (the dreamer is gmeow:Person)
    "slices/extensions/employment/examples/job.ttl", // gmeow:employmentType → shared EmploymentType individual untyped standalone; gmeow:membershipMember → Agent not typed standalone
    "slices/extensions/finance/examples/double-entry.ttl", // gmeow:ledgerAccountHolder → Agent not typed standalone
    "slices/extensions/graphrag/examples/lillith-dataset.ttl", // gmeow:licensedWork → InformationObject not typed standalone; gmeow:licensor → Agent not typed standalone
    "slices/extensions/graphrag/examples/lillith-pipeline.ttl", // gmeow:chunkOf/embeddingOf → InformationObject not typed standalone; gmeow:distanceMetric → shared DistanceMetric individual untyped standalone
    "slices/extensions/images/examples/photo-metadata.ttl", // gmeow:selectorType → shared selector-type individual untyped standalone
    "slices/extensions/lexicon/examples/word-etymology.ttl", // gmeow:derivationKind → shared DerivationKind individual untyped standalone; gmeow:derivationTarget/etymonSource → InformationObject not typed standalone
    "slices/extensions/music/examples/score-as-lossy-projection.ttl", // gmeow:realizes → Work not typed standalone
    "slices/extensions/notes/examples/annotations-and-notes.ttl", // gmeow:commentParent → Entity not typed standalone
    "slices/extensions/sensory/examples/sensor-reading.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/extensions/sensory-environment/examples/measured-vs-perceived.ttl", // gmeow:environmentAtLocation → Location not typed standalone (the room is gmeow:Place)
    // Bucket B — `sh:minCount` (MinCountConstraintComponent): the example omits a
    // P11-required reference / temporal frame on a value or interval. The frame
    // lives in the full ontology context; standalone the fixture is illustrative.
    "slices/core/creative-works/examples/wemi-novel.ttl", // Expression missing gmeow:hasReferenceFrame (P11)
    "slices/core/documents/examples/web-presence.ttl", // Expression missing gmeow:hasReferenceFrame (P11)
    "slices/core/learning/examples/skill-acquisition-trajectory.ttl", // TimeInterval missing gmeow:hasTemporalFrame (P11)
    "slices/core/affect/examples/two-critics.ttl", // Expression missing gmeow:hasReferenceFrame (P11)
    "slices/extensions/narrative/examples/flashback.ttl", // Event missing gmeow:eventTemporalFrame (P11)
    // Bucket A (lang: grounding graft) — the example references shared lang:/logic:
    // grounding individuals (rendering/denotation/preservation kinds, sign-system
    // kinds, scripts, the seed lang:english) that are typed in the grounding slices
    // but not standalone in the example's closed-world scope.
    "slices/core/coreference/examples/authority-links.ttl", // lang:denotationKind → lang:denotesEntity + lang:inSignSystem → lang:english (shared grounding individuals) untyped standalone
    "slices/core/language/examples/multilingual-document.ttl", // lang:renderingKind/transliterationScheme → shared rendering/transliteration individuals + lang:renderingPreservation → logic:PreservationKind (logic:ExactPreservation/SoundUnderApproximation) untyped standalone
    "slices/core/notation/examples/notation-systems.ttl", // lang:signSystemKind → lang:notationalKind + lang:modality → lang:writtenModality + lang:renderingPreservation → logic:PreservationKind untyped standalone
    "slices/core/notation/examples/pydantic-projection-profile.ttl", // lang:signSystemKind → lang:notationalKind + lang:modality → lang:writtenModality + gmeow:notationSystemKind → gmeow:symbolicKindEncoding + lang:renderingPreservation/logic:preservationKind → logic:ValidationOnly + the logic:Correspondence value individuals (logic:Overlaps/BridgeView/CommitmentShiftingBridge/Crisp) are shared individuals defined in module.ttl, untyped standalone
    "slices/grounding/math/examples/expression-rendering.ttl", // lang:renderingKind → lang:renderingNotation + lang:renderingPreservation → logic:ExactPreservation (shared logic:PreservationKind) untyped standalone
];

/// The repo root (two levels up from this crate's manifest dir).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// Load one Turtle data-graph file into a frozen [`RdfDataset`] via the native
/// codec — the SAME lenient native path the shape union uses
/// ([`shape_union::load_shapes`]).
fn load_data_graph(path: &Path) -> Arc<RdfDataset> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse_dataset(&bytes, "text/turtle", None)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Glob every `slices/*/*/examples/*.ttl`, sorted, as repo-relative paths.
fn example_files(repo: &Path) -> Vec<PathBuf> {
    let slices = repo.join("slices");
    let mut out: Vec<PathBuf> = Vec::new();
    for group in read_dirs(&slices) {
        for slice in read_dirs(&group) {
            let examples = slice.join("examples");
            if !examples.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&examples)
                .unwrap_or_else(|e| panic!("read {}: {e}", examples.display()))
            {
                let path = entry.expect("dir entry").path();
                if path.extension().and_then(|e| e.to_str()) == Some("ttl") && path.is_file() {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out
}

/// Immediate subdirectories of `dir`, sorted (empty when `dir` is absent).
fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        // Fail fast on an unreadable entry rather than silently dropping a
        // slice/example directory (which would let the sweep pass without covering
        // the full corpus) — matching the file-level `example_files()` behavior.
        .map(|e| {
            e.unwrap_or_else(|err| panic!("read {} entry: {err}", dir.display()))
                .path()
        })
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// Render a path relative to the repo root (forward slashes) for reports/allowlist.
fn rel(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Whether `dataset` conforms to the merged `shapes` per the native SHACL engine.
fn conforms_to_shacl(dataset: &Arc<RdfDataset>, shapes: &Shapes) -> bool {
    engine::validate_dataset(dataset.as_ref(), shapes)
        .expect("validate_dataset over a frozen dataset is infallible")
        .conforms
}

fn gmeow_namespaces() -> json_schema::Namespaces {
    json_schema::Namespaces::new(
        "gmeow",
        &[
            (
                "gmeow".to_owned(),
                "https://blackcatinformatics.ca/gmeow/".to_owned(),
            ),
            (
                "logic".to_owned(),
                "https://blackcatinformatics.ca/logic/".to_owned(),
            ),
            (
                "lang".to_owned(),
                "https://blackcatinformatics.ca/lang/".to_owned(),
            ),
            (
                "math".to_owned(),
                "https://blackcatinformatics.ca/math/".to_owned(),
            ),
        ],
    )
    .expect("gmeow namespaces")
}

#[test]
fn example_corpus_validates_against_closed_world_schema() {
    let repo = repo_root();

    // The merged shape union + the JSON Schema derived from those same shapes.
    let (_shapes_store, shapes) =
        shape_union::load_shapes(&repo).expect("load merged SHACL shapes");
    let compiled = json_schema::compile(&shapes, &gmeow_namespaces());
    let schema_bytes = compiled.schema_json.as_bytes();

    let non_conformant: std::collections::BTreeSet<&str> = NON_CONFORMANT.iter().copied().collect();

    let examples = example_files(&repo);
    assert!(
        !examples.is_empty(),
        "no example fixtures found under slices/*/*/examples/*.ttl"
    );

    // Per-example outcomes.
    let mut schema_failures: Vec<(String, Vec<String>)> = Vec::new();
    let mut shacl_failing: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut excluded_count = 0usize;
    let mut passed_count = 0usize;

    for path in &examples {
        let relpath = rel(&repo, path);
        let store = load_data_graph(path);

        // (A) Does the example conform to its SHACL shapes? If not, it is
        //     illustrative, not valid instance data → out of scope.
        let shacl_ok = conforms_to_shacl(&store, &shapes);
        if !shacl_ok {
            shacl_failing.insert(relpath.clone());
        }

        if non_conformant.contains(relpath.as_str()) {
            excluded_count += 1;
            continue;
        }

        // (B) Project to JSON-LD and validate against the closed-world schema.
        let instance_value = instance::project_graph(&store, &gmeow_namespaces());
        let instance_bytes = serde_json::to_vec(&instance_value).expect("serialize instance");
        let violations = validate_instance(&instance_bytes, InstanceFormat::Json, schema_bytes)
            .unwrap_or_else(|e| panic!("validate_instance hard error for {relpath}: {e}"));

        if violations.is_empty() {
            passed_count += 1;
        } else {
            schema_failures.push((relpath, violations));
        }
    }

    // Every swept example is EXACTLY one of passed / excluded / schema-failure —
    // a partition invariant (replaces a bare sweep-summary log line).
    assert_eq!(
        passed_count + excluded_count + schema_failures.len(),
        examples.len(),
        "sweep partition: {passed_count} passed + {excluded_count} excluded + {} schema-failures must total {} examples",
        schema_failures.len(),
        examples.len(),
    );

    // Invariant 1: the allowlist must be EXACTLY the SHACL-failing set, so an
    // exclusion can never silently mask a JSON-schema soundness bug.
    let allowlisted: std::collections::BTreeSet<String> =
        non_conformant.iter().map(|s| (*s).to_owned()).collect();
    if allowlisted != shacl_failing {
        let only_allowlist: Vec<&String> = allowlisted.difference(&shacl_failing).collect();
        let only_shacl: Vec<&String> = shacl_failing.difference(&allowlisted).collect();
        panic!(
            "NON_CONFORMANT allowlist drifted from the SHACL-failing set.\n\
             listed but actually SHACL-CONFORMANT (remove from allowlist): {only_allowlist:#?}\n\
             SHACL-NON-CONFORMANT but not listed (add to allowlist with a reason, \
             or fix the example): {only_shacl:#?}"
        );
    }

    // Invariant 2: every in-scope (SHACL-conformant, non-excluded) example must
    // validate against the closed-world JSON Schema.
    if !schema_failures.is_empty() {
        let mut report = String::from(
            "closed-world JSON Schema REJECTED SHACL-conformant example data \
             (soundness bug in emitter/projector):\n",
        );
        for (path, violations) in &schema_failures {
            report.push_str(&format!("\n{path}:\n"));
            for v in violations.iter().take(5) {
                report.push_str(&format!("  - {v}\n"));
            }
            if violations.len() > 5 {
                report.push_str(&format!("  … and {} more\n", violations.len() - 5));
            }
        }
        panic!("{report}");
    }
}
