// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// A minimal TBox: a two-step subclass chain. Reasoning propagates an individual's
// type up the chain, so an example asserting `a Dog` yields inferred `a Animal`,
// `a LivingThing` — the canonical "try it" shape, with no existentials (hence no
// Skolem witnesses) so the fixture is fully deterministic.
const EDB_TTL: &str = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:Dog rdfs:subClassOf ex:Animal .
ex:Animal rdfs:subClassOf ex:LivingThing .
";
const EX_DOG_TTL: &str = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
ex:rex a ex:Dog .
";
const EX_CAT_TTL: &str = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
ex:felix a ex:Animal .
";
const SLICE: &str = "https://example.org/gmeow-try-it/slice";

/// Normalize a display line so the golden pins WHICH inferences land WHERE, not the
/// non-stable identity of blank / content-addressed Skolem witnesses. Blank labels
/// collapse to `_:_`; any Skolem-witness IRI collapses to `<skolem>`. (The fixture is
/// witness-free by design, so this is a defensive no-op here — present so the golden
/// is robust if a future reasoning path starts materializing witnesses.)
fn norm(line: &str) -> String {
    line.split(' ')
        .map(|tok| {
            if tok.starts_with("_:") {
                "_:_".to_string()
            } else if tok.contains("blackcatinformatics.ca/gmeow/skolem/") {
                "<skolem>".to_string()
            } else {
                tok.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn norm_all(lines: &[String]) -> Vec<String> {
    let mut v: Vec<String> = lines.iter().map(|l| norm(l)).collect();
    v.sort();
    v.dedup();
    v
}

fn compute() -> gmeow_docs::ExecutableDocsData {
    let edb = parse_dataset(EDB_TTL.as_bytes(), "text/turtle", None).expect("parse EDB");
    // The base ontology-only closure = reason(EDB), exactly as the reason stage
    // commits it — subtracted so only EXAMPLE-INDUCED inferences survive.
    let base = crate::stages::reason::reason_test_dataset(edb.as_ref()).expect("reason base");
    let sources = vec![
        ExampleSource {
            slice: SLICE.to_string(),
            logical_path: "examples/dog.ttl".to_string(),
            text: EX_DOG_TTL.to_string(),
        },
        ExampleSource {
            slice: SLICE.to_string(),
            logical_path: "examples/cat.ttl".to_string(),
            text: EX_CAT_TTL.to_string(),
        },
    ];
    executable_docs_from_sources(edb.as_ref(), base.closure.as_bytes(), &sources)
        .expect("executable docs")
}

#[test]
fn try_it_attribution_is_semantically_pinned() {
    const NS: &str = "https://example.org/gmeow-try-it/";
    let data = compute();

    // ── Per-example attribution: each example's OWN subject carries its induced
    //    inferences; the told triple stays in `asserted`, never `inferred`. ──
    assert_eq!(
        data.example_inferences.len(),
        2,
        "both examples induce an inference"
    );
    let dog = data
        .example_inferences
        .get(&gmeow_docs::example_key(SLICE, "examples/dog.ttl"))
        .expect("dog example diff");
    assert_eq!(
        norm_all(&dog.asserted),
        vec![format!("<{NS}rex> rdf:type <{NS}Dog>")]
    );
    assert_eq!(
        norm_all(&dog.inferred),
        vec![
            format!("<{NS}rex> rdf:type <{NS}Animal>"),
            format!("<{NS}rex> rdf:type <{NS}LivingThing>"),
        ]
    );
    let cat = data
        .example_inferences
        .get(&gmeow_docs::example_key(SLICE, "examples/cat.ttl"))
        .expect("cat example diff");
    assert_eq!(
        norm_all(&cat.asserted),
        vec![format!("<{NS}felix> rdf:type <{NS}Animal>")]
    );
    assert_eq!(
        norm_all(&cat.inferred),
        vec![format!("<{NS}felix> rdf:type <{NS}LivingThing>")]
    );

    // ── No inference in this fixture is unattributable — the cross-example bucket
    //    is empty (there are no shared / Skolem-witness inferences here). ──
    assert!(
        norm_all(&data.cross_example).is_empty(),
        "no unattributable inferences: got {:?}",
        norm_all(&data.cross_example)
    );

    // The playground-asset assertion that stood here is gone with the asset. It pinned
    // `playground_trig` to `documentation(∅) ∪ base closure` — a real property of a
    // projection that no longer exists, because the playground queries the bundle
    // directly. What that projection was FOR is now pinned where it belongs: by
    // `crates/docs/tests/shipped_queries_execute.rs`, which runs the queries the page
    // actually ships against the real bundle and fails on an empty result. That is a
    // stronger guard — the old one could pass over an asset no shipped query matched,
    // which is exactly what it did.

    // ── Determinism: the core is a pure function of its inputs. ──
    let again = compute();
    assert_eq!(
        data.example_inferences, again.example_inferences,
        "attribution must be deterministic"
    );
    assert_eq!(
        data.cross_example, again.cross_example,
        "cross_example must be deterministic"
    );
}

/// The witness-insensitive subtraction: an ONTOLOGY-level Skolem-witness edge whose
/// content-addressed IRI differs between the reduced-seed reasoning context and the
/// committed base closure must still cancel against the base (never leak into
/// `cross_example`), while an example-SUBJECT fact (absent from the base) survives.
/// Without normalization the context-shifted witness IRI fails to match and pollutes
/// the bucket — the exact divergence the full-EDB-vs-reduced-seed validation surfaced
/// on the real ontology.
#[test]
fn ontology_witness_cancels_across_skolem_iri_shift() {
    // Seed: C ⊑ Mid ⊑ <skolem/aaa> — transitivity DERIVES `C ⊑ <skolem/aaa>`, a
    // non-example-subject witness edge (a cross_example candidate).
    let seed_ttl = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:C rdfs:subClassOf ex:Mid .
ex:Mid rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/skolem/aaa> .
";
    let seed = parse_dataset(seed_ttl.as_bytes(), "text/turtle", None).expect("parse seed");
    // The committed base closure carries the SAME edge under a DIFFERENT skolem IRI.
    let base_ttl = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:C rdfs:subClassOf <https://blackcatinformatics.ca/gmeow/skolem/bbb> .
";
    let sources = vec![ExampleSource {
        slice: SLICE.to_string(),
        logical_path: "examples/probe.ttl".to_string(),
        text: "@prefix ex: <https://example.org/gmeow-try-it/> .\nex:x a ex:C .\n".to_string(),
    }];
    let data = executable_docs_from_sources(seed.as_ref(), base_ttl.as_bytes(), &sources)
        .expect("executable docs");

    // The ontology witness edge (C ⊑ <skolem/aaa>) is SUBTRACTED despite the base
    // carrying it under <skolem/bbb> — so it never reaches cross_example.
    assert!(
        !data
            .cross_example
            .iter()
            .any(|l| l.contains("/C>") && l.contains("skolem")),
        "context-shifted ontology witness leaked into cross_example: {:?}",
        data.cross_example
    );
    // The example subject still receives its derived type (x a C told; x a Mid derived).
    let probe = data
        .example_inferences
        .get(&gmeow_docs::example_key(SLICE, "examples/probe.ttl"))
        .expect("probe example diff");
    assert!(
        probe
            .inferred
            .iter()
            .any(|l| l.contains("/x>") && l.contains("/Mid>")),
        "example subject must still receive its derived type: {:?}",
        probe.inferred
    );
}

/// Two DIFFERENT worked examples naming the SAME subject IRI are ambiguous: neither
/// example can be said to have solely induced an inference on that shared subject, so
/// the induced inferences must route to `cross_example`, never to either example's
/// `.inferred` (a plain last-write-wins map would misattribute them to whichever
/// example happened to be parsed last).
#[test]
fn shared_subject_across_examples_routes_to_cross_example() {
    const NS: &str = "https://example.org/gmeow-try-it/";
    let edb = parse_dataset(EDB_TTL.as_bytes(), "text/turtle", None).expect("parse EDB");
    let base = crate::stages::reason::reason_test_dataset(edb.as_ref()).expect("reason base");
    let shared_ttl = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
ex:shared a ex:Dog .
";
    let sources = vec![
        ExampleSource {
            slice: SLICE.to_string(),
            logical_path: "examples/shared-a.ttl".to_string(),
            text: shared_ttl.to_string(),
        },
        ExampleSource {
            slice: SLICE.to_string(),
            logical_path: "examples/shared-b.ttl".to_string(),
            text: shared_ttl.to_string(),
        },
    ];
    let data = executable_docs_from_sources(edb.as_ref(), base.closure.as_bytes(), &sources)
        .expect("executable docs");

    // Both `ex:shared a ex:Animal` and `ex:shared a ex:LivingThing` are induced by an
    // asserted `ex:shared a ex:Dog`, but which example "owns" `ex:shared` is
    // ambiguous — they must land in cross_example, not in either example's diff.
    let expected_cross = vec![
        format!("<{NS}shared> rdf:type <{NS}Animal>"),
        format!("<{NS}shared> rdf:type <{NS}LivingThing>"),
    ];
    assert_eq!(
        norm_all(&data.cross_example),
        expected_cross,
        "ambiguous shared-subject inferences must route to cross_example: {:?}",
        norm_all(&data.cross_example)
    );

    // Neither example's diff carries the ambiguous inferences (both would be
    // present, and only their own `asserted` line, if attribution were unambiguous
    // — but a shared subject must never appear in a per-example `.inferred`).
    for key in [
        gmeow_docs::example_key(SLICE, "examples/shared-a.ttl"),
        gmeow_docs::example_key(SLICE, "examples/shared-b.ttl"),
    ] {
        if let Some(diff) = data.example_inferences.get(&key) {
            assert!(
                diff.inferred.is_empty(),
                "shared-subject inference must not be attributed to a single example {key}: {:?}",
                diff.inferred
            );
        }
    }
}

/// `build_executable_docs_data`'s core correctness claim: reasoning the reduced seed
/// `source_load_dataset(upstream).project_named_graph(GRAPH_AUTHORED_DEFAULT)`
/// reproduces the attribution `assemble_object_level_edb`'s FULL object-level EDB
/// would give, because worked examples parse into the default world and the calculus
/// is same-world (imports/statements/alignments/logic ride NAMED worlds the examples
/// cannot reach). This mirrors `assemble_object_level_edb`'s real shape (carrier.rs
/// ~515-556): the authored-default content is projected OUT of its internal transport
/// tag into the true default graph, then UNIONED with the other pipeline products,
/// each rooted in ITS OWN named-world graph (never the default graph).
///
/// The fixture is DISCRIMINATING: the full EDB's "import" named world carries an
/// axiom (`ex:Animal rdfs:subClassOf ex:ImportedExtra`) that WOULD transitively fire
/// on the example's own asserted type — yielding `ex:rex a ex:ImportedExtra` — if it
/// were (wrongly) merged into the default world the examples inhabit; a control
/// computation below reasons the SAME axioms flattened into one world to prove that.
/// In the real (world-separated) fixture that axiom lives only in the full seed's
/// `import` named graph, structurally absent from the reduced (authored-default
/// projection) seed — so if the reduced-seed optimization, or the reasoner's
/// world-scoping it relies on, were unsound, this test would see `reduced != full`
/// or the `ImportedExtra` type leaking into an example's `.inferred`.
#[test]
fn reduced_seed_attribution_matches_full_edb_attribution() {
    // Mirrors what `source_load_dataset(upstream)` carries: the authored default-
    // world chain under the GRAPH_AUTHORED_DEFAULT internal transport tag.
    let raw_source_load_trig = format!(
        "@prefix ex: <https://example.org/gmeow-try-it/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             GRAPH <{GRAPH_AUTHORED_DEFAULT}> {{\n\
             \x20 ex:Dog rdfs:subClassOf ex:Animal .\n\
             \x20 ex:Animal rdfs:subClassOf ex:LivingThing .\n\
             }}\n"
    );
    let raw_source_load = parse_dataset(raw_source_load_trig.as_bytes(), "application/trig", None)
        .expect("parse raw source-load fixture");
    // The reduced seed: exactly `build_executable_docs_data`'s
    // `source_load_dataset(upstream)?.project_named_graph(GRAPH_AUTHORED_DEFAULT)`
    // call — the authored chain re-rooted into the true default graph.
    let reduced_seed = raw_source_load.project_named_graph(GRAPH_AUTHORED_DEFAULT);

    // A SEPARATE named "import" world (standing in for GRAPH_IMPORTS /
    // GRAPH_ALIGNMENTS / the logic graphs `assemble_object_level_edb` unions in),
    // carrying an additional superclass edge off `ex:Animal` that only a
    // world-isolation bug would let leak into the default-world reasoning the
    // examples participate in.
    let import_world_trig = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
GRAPH <https://example.org/gmeow-try-it/import> {
  ex:Animal rdfs:subClassOf ex:ImportedExtra .
}
";
    let import_world = parse_dataset(import_world_trig.as_bytes(), "application/trig", None)
        .expect("parse import-world fixture");

    // Control computation proving the fixture is DISCRIMINATING: flatten the SAME
    // authored-chain + import axioms into a single (default) world and reason over
    // `(flat_edb ∪ the dog example)` directly — no reduced/full split at all. If
    // `ex:Animal rdfs:subClassOf ex:ImportedExtra` were reachable from a default-
    // world example, this is where it would show up.
    let flat_probe_ttl = "\
@prefix ex: <https://example.org/gmeow-try-it/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:Dog rdfs:subClassOf ex:Animal .
ex:Animal rdfs:subClassOf ex:LivingThing .
ex:Animal rdfs:subClassOf ex:ImportedExtra .
ex:rex a ex:Dog .
";
    let flat_probe = parse_dataset(flat_probe_ttl.as_bytes(), "text/turtle", None)
        .expect("parse flat probe fixture");
    let flat_reasoned =
        crate::stages::reason::reason_test_dataset(flat_probe.as_ref()).expect("reason flat probe");
    assert!(
        flat_reasoned.closure.contains("ImportedExtra"),
        "fixture is not discriminating: merging the import axiom into the default \
             world never yields an ImportedExtra inference on the example subject: {}",
        flat_reasoned.closure
    );

    // The full object-level EDB, shaped exactly like `assemble_object_level_edb`:
    // the default-world projection UNIONED with the other named-world graphs.
    let full_edb = purrdf::RdfDataset::union(&[&reduced_seed, import_world.as_ref()]);

    // What stage-reason would commit for this full EDB: the base ontology-only
    // closure, subtracted (witness-insensitively) in both runs below. Because the
    // reasoner is world-scoped by design (PIPELINE_SPINE's same-world calculus),
    // this closure does NOT show the import axiom crossing into the default world —
    // that is exactly the invariant this test locks down, via the reduced-vs-full
    // attribution comparison below rather than via this closure alone.
    let base = crate::stages::reason::reason_test_dataset(&full_edb).expect("reason full-EDB base");

    let sources = vec![
        ExampleSource {
            slice: SLICE.to_string(),
            logical_path: "examples/dog.ttl".to_string(),
            text: EX_DOG_TTL.to_string(),
        },
        ExampleSource {
            slice: SLICE.to_string(),
            logical_path: "examples/cat.ttl".to_string(),
            text: EX_CAT_TTL.to_string(),
        },
    ];

    // Run 1 — production behavior: reason the REDUCED seed (the authored
    // default-world projection alone), exactly as `build_executable_docs_data` does.
    let reduced = executable_docs_from_sources(&reduced_seed, base.closure.as_bytes(), &sources)
        .expect("executable docs (reduced seed)");

    // Run 2 — the ground truth: reason the FULL object-level EDB, import world and
    // all, exactly as `assemble_object_level_edb` + stage-reason would.
    let full = executable_docs_from_sources(&full_edb, base.closure.as_bytes(), &sources)
        .expect("executable docs (full EDB)");

    // The reduction must be attribution-lossless: same per-example diffs, same
    // cross-example bucket. Normalize with the module's witness-insensitive `norm_all`
    // — this fixture has no existentials so it is a no-op here, but keeps the
    // comparison robust to incidental witness IRIs.
    let reduced_keys: Vec<&String> = reduced.example_inferences.keys().collect();
    let full_keys: Vec<&String> = full.example_inferences.keys().collect();
    assert_eq!(
        reduced_keys, full_keys,
        "reduced- and full-seed runs must attribute to the same set of examples"
    );
    for key in full.example_inferences.keys() {
        let reduced_diff = reduced
            .example_inferences
            .get(key)
            .unwrap_or_else(|| panic!("reduced run missing diff for {key}"));
        let full_diff = &full.example_inferences[key];
        assert_eq!(
            norm_all(&reduced_diff.asserted),
            norm_all(&full_diff.asserted),
            "asserted lines diverged for {key}"
        );
        assert_eq!(
            norm_all(&reduced_diff.inferred),
            norm_all(&full_diff.inferred),
            "reduced-seed attribution diverged from full-EDB attribution for {key}"
        );
    }
    assert_eq!(
        norm_all(&reduced.cross_example),
        norm_all(&full.cross_example),
        "cross_example diverged between reduced- and full-seed runs"
    );

    // Neither run's example diffs pick up the import-world's `ex:ImportedExtra` edge
    // — the flat probe above proved it WOULD if the worlds merged, so its absence
    // here is the world boundary holding, not the fixture being inert.
    for diff in full
        .example_inferences
        .values()
        .chain(reduced.example_inferences.values())
    {
        assert!(
            diff.inferred.iter().all(|l| !l.contains("ImportedExtra")),
            "import-world axiom leaked into example attribution: {:?}",
            diff.inferred
        );
    }
    assert!(
        reduced
            .cross_example
            .iter()
            .chain(full.cross_example.iter())
            .all(|l| !l.contains("ImportedExtra")),
        "import-world axiom leaked into cross_example"
    );
}
