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

use gmeow_validate::instance::{validate_instance, InstanceFormat};
use purrdf::shapes::shapes::Shapes;
use purrdf::shapes::{engine, instance, json_schema, shape_union};

use purrdf::parse_dataset;
use purrdf::RdfDataset;

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
    "slices/core/inquiry/examples/loaded-question.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inquiry/examples/open-question-and-resolution.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/grounding/lang/examples/forms-and-sign-systems.ttl", // lang:partOfSpeech/slotRole/featureKey/featureValue/analysisLevel/compositionLevel/offsetSpace/signSystemKind/modality/grammarFormalism → shared inventory, role, level, and kind individuals (noun, subjectRole, featNumber, valPlur, parsedLevel, sentenceLevel, codepointOffset, naturalLanguageKind, writtenModality, ebnfFormalism) defined in module.ttl, untyped standalone
    "slices/grounding/logic/examples/formalization-governance.ttl", // logic:candidateCategory/candidateProjectionBehavior/candidateNonEntailment → shared governance individuals (categories, preservation kinds, the standing obligations) defined in module.ttl, untyped standalone
    "slices/grounding/math/examples/measure-and-dimension.ttl", // math:exponentOfDimension → the shared SI base-dimension individuals (massDimension/lengthDimension/timeDimension) defined in module.ttl, untyped standalone; math:withRespectTo/hasDimension sh:class Measure/Dimension → the subclass-typed measure and dimension nodes (LebesgueMeasure/DerivedDimension) lack the subClassOf chain standalone
    "slices/grounding/math/examples/numbers-sets-functions.ttl", // math:hasElement → set-member individuals (two/three/five/seven) untyped standalone; math:memberCondition → a logic:Formula node (no closed-world schema entry, the denotation seam)
    "slices/core/metacognition/examples/dunning-kruger.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/metacognition/examples/reflection-revision.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/names/examples/person-names.ttl", // gmeow:usageAppellation/usageNamed → Appellation/Entity not typed standalone
    "slices/core/observations/examples/blood-pressure.ttl", // gmeow:observationMethod (methodInstrumentalReading) + the reference frame's shared component individuals (determinacyCrisp, frameKindScalar, frameRealmMeasurement, axisScalar) untyped standalone
    "slices/core/observations/examples/temperature-reading.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/pipeline/examples/minimal-pipeline.ttl", // gmeow:hasCapability → shared gmeow:StageCapability untyped standalone
    "slices/core/places/examples/located-place.ttl", // gmeow:vantage → Agent not typed standalone (the survey team is gmeow:Organization)
    "slices/core/profiles/examples/named-profile-membership.ttl", // gmeow:profileAppliesTo → owl:Class target not typed standalone
    "slices/core/quality/examples/dataset-completeness.ttl", // gmeow:assessedEntity → Entity not typed standalone (the dataset is gmeow:Dataset)
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

    // Log a sweep summary (visible with --nocapture).
    eprintln!(
        "example sweep: {} total, {} passed, {} excluded (non-conformant), {} schema failures",
        examples.len(),
        passed_count,
        excluded_count,
        schema_failures.len()
    );
    if !non_conformant.is_empty() {
        eprintln!("excluded (SHACL-non-conformant, out of scope):");
        for ex in NON_CONFORMANT {
            eprintln!("  - {ex}");
        }
    }

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
