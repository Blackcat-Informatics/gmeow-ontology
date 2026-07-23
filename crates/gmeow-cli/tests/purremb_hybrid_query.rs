// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that the SHIPPED `gmeow` binary exposes a verified
//! PURREMB retrieval mode: `gmeow hybrid-query --purremb <artifact> --source
//! <pack> ...`. A tiny valid `.purremb` artifact of RDF-1.2 IRI targets is built
//! with PurRDF's public writer APIs, written to tempfiles with its exact source
//! pack, and driven end-to-end through the real `Cli`/`Commands::HybridQuery`
//! clap dispatch via `assert_cmd`.
//!
//! A well-formed query prints resolved answers plus both the standard query
//! receipt AND the PURREMB retrieval receipt naming every contributing PURREMB
//! identity. A verification mismatch (perturbed source bytes) or a bad selection
//! identity (a wrong stored-matrix identity) exits NON-ZERO and prints no
//! answers — never a silently empty completed result.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use purrdf::{
    AppliedStage, ArtifactIdentity, ArtifactIdentityKind, CanonicalMetadataInput,
    CertifiedPurrpckSource, ContentDigest, DimensionalityPolicy, DistanceMetric, EmbeddingBuilder,
    EmbeddingFamilyContract, EmbeddingView, MatrixInput, MatrixRow, PrefixPostprocessing,
    ProjectionSpec, RdfDatasetBuilder, RdfTermTarget, StageImplementation, TargetSet, VectorDtype,
    verify_embedding,
};

/// Shared IRI namespace: `ex:` expands to this in the query program, so a target
/// `ex:e/a` reconstructs to `<https://example.org/e/a>` and the relation
/// `ex:rel/near` names the registered provider relation.
const NS: &str = "https://example.org/";

/// The three in-corpus IRI local names (under `NS`) the fixture embeds.
const LOCALS: [&str; 3] = ["e/a", "e/b", "e/c"];

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

fn artifact(name: &str) -> ArtifactIdentity {
    ArtifactIdentity::new(
        format!("{NS}artifact/{name}"),
        "application/octet-stream",
        ContentDigest::of(name.as_bytes()),
        None,
        ArtifactIdentityKind::Single,
    )
    .expect("artifact identity")
}

fn stage(name: &str) -> AppliedStage {
    AppliedStage::Applied(
        StageImplementation::new(
            format!("{NS}stage/{name}"),
            ContentDigest::of(name.as_bytes()),
            "application/cbor",
            vec![0xa1, 0x01, 0x01],
        )
        .expect("stage implementation"),
    )
}

/// A minimal cosine, f32, fixed-dimension-2 family contract.
fn family_contract() -> EmbeddingFamilyContract {
    EmbeddingFamilyContract {
        model: artifact("model"),
        engine: artifact("engine"),
        tokenizer: artifact("tokenizer"),
        execution: stage("execution"),
        subject_projection: stage("iri-projection"),
        preprocessing: AppliedStage::NotApplied,
        chunking: AppliedStage::NotApplied,
        pooling: stage("pooling"),
        normalization: AppliedStage::NotApplied,
        truncation: AppliedStage::NotApplied,
        dtype: VectorDtype::F32,
        metric: DistanceMetric::Cosine,
        dimensionality: DimensionalityPolicy::fixed(2, PrefixPostprocessing::None)
            .expect("fixed dimension"),
        extensions: Vec::new(),
    }
}

/// The introspected identities a query selection must name, all lowercase hex.
struct FixtureIdentities {
    target_set: String,
    family: String,
    vector_space: String,
    matrix: String,
}

/// A built PURREMB artifact plus its exact source pack and the identities a
/// query must declare against it.
struct Fixture {
    artifact_bytes: Vec<u8>,
    source_bytes: Vec<u8>,
    identities: FixtureIdentities,
}

/// Build a tiny valid `.purremb` artifact of three RDF-1.2 IRI targets over a
/// distinct-vector cosine matrix, then introspect its verified view for the
/// selection identities. Every IRI target reconstructs losslessly to its IRI, so
/// a bound in-corpus IRI query resolves and the retrieved rows map back to IRIs.
fn build_fixture() -> Fixture {
    // An empty certified RDF source pack: the IRI targets carry no source-local
    // ordinal, so certified verification never cross-checks them against the pack
    // — only the exact source digest, certified RDF digest, and dataset target.
    let dataset = RdfDatasetBuilder::new()
        .freeze()
        .expect("empty RDF dataset");
    let (source, source_bytes) =
        CertifiedPurrpckSource::from_dataset(&dataset).expect("certified source pack");
    let dataset_target = source.dataset_target(true).expect("dataset target");

    let contract = family_contract();
    let family = contract.derive().expect("derived family");

    let iri_targets: Vec<_> = LOCALS
        .iter()
        .map(|local| {
            RdfTermTarget::Iri(format!("{NS}{local}"))
                .into_target(true, None)
                .expect("iri target")
        })
        .collect();

    let set =
        TargetSet::new(iri_targets.iter().map(|target| target.id).collect()).expect("target set");

    let mut targets = vec![dataset_target];
    targets.extend(iri_targets.iter().cloned());

    let metadata = CanonicalMetadataInput {
        source,
        family_contracts: vec![contract],
        targets,
        target_sets: vec![set.clone()],
        relations: Vec::new(),
        token_spans: Vec::new(),
        external_bindings: Vec::new(),
        indexes: Vec::new(),
        extensions: Vec::new(),
    };

    // Distinct, non-zero rows so cosine distances are finite and the ordering is
    // deterministic. One row per set target id.
    let vectors = [[1.0_f32, 2.0], [2.0, 1.0], [3.0, 1.0]];
    let rows = iri_targets
        .iter()
        .zip(vectors)
        .map(|(target, vector)| MatrixRow::new(target.id, vector.to_vec()))
        .collect();
    let matrix = MatrixInput {
        family_id: family.id,
        target_set_id: set.id,
        stored_dimension: 2,
        rows,
        projections: vec![ProjectionSpec::derive(
            family.id,
            2,
            PrefixPostprocessing::None,
        )],
    };

    let mut builder = EmbeddingBuilder::from_typed_metadata(metadata);
    builder.add_f32_matrix(matrix);
    let artifact_bytes = builder.build().expect("built PURREMB artifact").bytes;

    // Introspect the verified view for the exact stored identities a selection
    // must name (never guessed — read back from the artifact GMEOW ships).
    let mut view = EmbeddingView::from_bytes(&artifact_bytes).expect("structural view");
    verify_embedding(&mut view).expect("verified view");
    let matrix_view = view.matrices().next().expect("one stored matrix");
    let vector_space = view
        .projections()
        .next()
        .expect("one effective projection")
        .vector_space_id();
    let identities = FixtureIdentities {
        target_set: matrix_view.target_set_id().to_hex(),
        family: matrix_view.family_id().to_hex(),
        vector_space: vector_space.to_hex(),
        matrix: matrix_view.id().to_hex(),
    };

    Fixture {
        artifact_bytes,
        source_bytes,
        identities,
    }
}

/// The query program: an IDB rule fires the PURREMB relation for the bound
/// in-corpus query IRI, and the goal binds every retrieved candidate IRI.
fn program_src() -> String {
    format!(
        ":- prefix(ex, '{NS}').\n\
         ex:near(Q, C) :- ex:rel/near(Q, C).\n\
         ?- ex:near(ex:e/a, C).\n"
    )
}

/// A single harmless asserted fact so the query world snapshot is non-trivial;
/// it does not participate in the retrieval join.
fn facts_ttl() -> String {
    format!("<{NS}x> <{NS}p> <{NS}y> .\n")
}

/// Stage the artifact, source, program, and facts into a fresh temp dir and
/// return their paths.
fn staged(fixture: &Fixture) -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let purremb = tmp.path().join("corpus.purremb");
    let source = tmp.path().join("corpus.purrpck");
    let program = tmp.path().join("query.logic");
    let facts = tmp.path().join("facts.ttl");
    std::fs::write(&purremb, &fixture.artifact_bytes).expect("write artifact");
    std::fs::write(&source, &fixture.source_bytes).expect("write source");
    std::fs::write(&program, program_src()).expect("write program");
    std::fs::write(&facts, facts_ttl()).expect("write facts");
    (tmp, purremb, source, program, facts)
}

const RELATION: &str = "https://example.org/rel/near";

/// A well-formed verified-PURREMB query prints answers and both receipts naming
/// the contributing PURREMB identities.
#[test]
fn purremb_hybrid_query_prints_answers_and_retrieval_receipt() {
    let fixture = build_fixture();
    let ids = &fixture.identities;
    let (_tmp, purremb, source, program, facts) = staged(&fixture);

    gmeow()
        .arg("hybrid-query")
        .arg("--facts")
        .arg(&facts)
        .arg("--program")
        .arg(&program)
        .arg("--purremb")
        .arg(&purremb)
        .arg("--source")
        .arg(&source)
        .args(["--relation", RELATION])
        .args(["--target-set", &ids.target_set])
        .args(["--family", &ids.family])
        .args(["--vector-space", &ids.vector_space])
        .args(["--matrix", &ids.matrix])
        .args(["--metric", "cosine"])
        .args(["--effective-dimension", "2"])
        .args(["--dtype", "f32"])
        .args(["--postprocessing", "none"])
        .args(["--retrieval-policy", "exact-full-space"])
        .args(["--source-mode", "certified"])
        .assert()
        .success()
        .stdout(
            // The bound query is itself an in-corpus row, so it is returned at
            // distance zero — at least one answer binds C.
            predicate::str::contains("answer C=<https://example.org/e/a>")
                .and(predicate::str::contains("annotation-distance="))
                .and(predicate::str::contains("status Ok"))
                // The standard query receipt names the PURREMB relation and marks
                // the provider as having contributed.
                .and(predicate::str::contains(format!("relation={RELATION}")))
                .and(predicate::str::contains("status=Complete"))
                .and(predicate::str::contains("contributed=true"))
                // The PURREMB retrieval receipt names every contributing identity.
                .and(predicate::str::contains("purremb-receipt"))
                .and(predicate::str::contains(format!("matrix={}", ids.matrix)))
                .and(predicate::str::contains(format!(
                    "target-set={}",
                    ids.target_set
                )))
                .and(predicate::str::contains(format!("family={}", ids.family)))
                .and(predicate::str::contains(format!(
                    "vector-space={}",
                    ids.vector_space
                )))
                .and(predicate::str::contains("policy=exact-full-space"))
                .and(predicate::str::contains("source-mode=certified")),
        );
}

/// Perturbed source bytes fail the source-pack verification: the binding does not
/// open, the query exits NON-ZERO, and no answer is ever printed.
#[test]
fn purremb_hybrid_query_perturbed_source_fails_with_no_answers() {
    let mut fixture = build_fixture();
    // Flip one source byte: same length, so this trips the exact source-digest
    // check rather than a length mismatch.
    let last = fixture.source_bytes.len() - 1;
    fixture.source_bytes[last] ^= 0xff;
    let ids = fixture.identities;
    let (_tmp, purremb, source, program, facts) = staged(&Fixture {
        artifact_bytes: fixture.artifact_bytes,
        source_bytes: fixture.source_bytes,
        identities: FixtureIdentities {
            target_set: ids.target_set.clone(),
            family: ids.family.clone(),
            vector_space: ids.vector_space.clone(),
            matrix: ids.matrix.clone(),
        },
    });

    gmeow()
        .arg("hybrid-query")
        .arg("--facts")
        .arg(&facts)
        .arg("--program")
        .arg(&program)
        .arg("--purremb")
        .arg(&purremb)
        .arg("--source")
        .arg(&source)
        .args(["--relation", RELATION])
        .args(["--target-set", &ids.target_set])
        .args(["--family", &ids.family])
        .args(["--vector-space", &ids.vector_space])
        .args(["--matrix", &ids.matrix])
        .args(["--metric", "cosine"])
        .args(["--effective-dimension", "2"])
        .args(["--dtype", "f32"])
        .args(["--postprocessing", "none"])
        .args(["--retrieval-policy", "exact-full-space"])
        .args(["--source-mode", "certified"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("answer ").not());
}

/// A wrong stored-matrix identity does not resolve against the opened artifact:
/// the selection is rejected closed, exiting NON-ZERO with no answers.
#[test]
fn purremb_hybrid_query_wrong_matrix_identity_fails_with_no_answers() {
    let fixture = build_fixture();
    let ids = &fixture.identities;
    let (_tmp, purremb, source, program, facts) = staged(&fixture);
    // A syntactically valid but absent 32-byte identity.
    let bogus_matrix = "00".repeat(32);

    gmeow()
        .arg("hybrid-query")
        .arg("--facts")
        .arg(&facts)
        .arg("--program")
        .arg(&program)
        .arg("--purremb")
        .arg(&purremb)
        .arg("--source")
        .arg(&source)
        .args(["--relation", RELATION])
        .args(["--target-set", &ids.target_set])
        .args(["--family", &ids.family])
        .args(["--vector-space", &ids.vector_space])
        .args(["--matrix", &bogus_matrix])
        .args(["--metric", "cosine"])
        .args(["--effective-dimension", "2"])
        .args(["--dtype", "f32"])
        .args(["--postprocessing", "none"])
        .args(["--retrieval-policy", "exact-full-space"])
        .args(["--source-mode", "certified"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("answer ").not());
}
