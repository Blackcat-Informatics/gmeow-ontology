// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gap C acceptance test over the SHIPPED bundle.
//!
//! AC4 ("verified non-vacuous") is committed here as a guard, not prose: this test folds the
//! committed `generated/dist/gmeow.gts` and asserts its `graph/goal-directed` named graph is
//! present and carries the PROOF-CHECKED answers/verdicts that trace back to the authored
//! `slices/grounding/logic/examples/reasoning-programs.ttl` cell — the same content the
//! passing stage test (`crates/pipeline/src/stages/goal_directed.rs`'s
//! `goal_directed_stage_attaches_a_nonempty_goal_directed_graph`) asserts over the in-memory
//! carrier, but here asserted over the COMMITTED artifact a regression could silently empty
//! without any other gate noticing.

use std::path::{Path, PathBuf};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/goal-directed";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The ground triples (subject, predicate, object as IRI/label strings) of ONE named graph of
/// the committed `gmeow.gts`, read through the kernel GTS reader.
fn graph_triples(graph_iri: &str) -> Vec<(String, String, String)> {
    let bytes =
        std::fs::read(repo_root().join("generated/dist/gmeow.gts")).expect("committed gmeow.gts");
    let g = purrdf::gts::read_graph(&bytes, true).expect("read_graph");
    let term = |id: usize| -> String {
        g.terms
            .get(id)
            .and_then(|t| t.value.clone())
            .unwrap_or_else(|| format!("<term {id}>"))
    };
    let mut out = Vec::new();
    for &(s, p, o, gname) in &g.quads {
        let Some(gid) = gname else { continue };
        if term(gid) != graph_iri {
            continue;
        }
        out.push((term(s), term(p), term(o)));
    }
    out
}

/// Objects `o` such that `(subject, predicate, o)` is present.
fn objects_of<'a>(
    triples: &'a [(String, String, String)],
    subject: &str,
    predicate: &str,
) -> Vec<&'a str> {
    triples
        .iter()
        .filter(|(s, p, _)| s == subject && p == predicate)
        .map(|(_, _, o)| o.as_str())
        .collect()
}

/// The query node IRI for one goal-directed demonstrator (matches
/// `gmeow_logic::goal_directed::query_iri`'s `"{GMEOW}goal-directed/{name}"` minting).
fn query_iri(name: &str) -> String {
    format!("{GMEOW}goal-directed/{name}")
}

/// The rendered `goalDirectedAtom` of every answer reachable from `name`'s query node via
/// `hasGoalDirectedAnswer`.
fn answers_of(triples: &[(String, String, String)], name: &str) -> Vec<String> {
    let q = query_iri(name);
    let has_answer = format!("{GMEOW}hasGoalDirectedAnswer");
    let atom_pred = format!("{GMEOW}goalDirectedAtom");
    objects_of(triples, &q, &has_answer)
        .into_iter()
        .flat_map(|answer| objects_of(triples, answer, &atom_pred))
        .map(str::to_owned)
        .collect()
}

/// The rendered `goalDirectedBinding` ("<var> = <surface>") of every answer reachable from
/// `name`'s query node via `hasGoalDirectedAnswer`.
fn bindings_of(triples: &[(String, String, String)], name: &str) -> Vec<String> {
    let q = query_iri(name);
    let has_answer = format!("{GMEOW}hasGoalDirectedAnswer");
    let binding_pred = format!("{GMEOW}goalDirectedBinding");
    objects_of(triples, &q, &has_answer)
        .into_iter()
        .flat_map(|answer| objects_of(triples, answer, &binding_pred))
        .map(str::to_owned)
        .collect()
}

/// The `(goalDirectedVerdictAtom, goalDirectedVerdict)` pairs of every verdict reachable from
/// `name`'s query node via `hasGoalDirectedVerdict`.
fn verdicts_of(triples: &[(String, String, String)], name: &str) -> Vec<(String, String)> {
    let q = query_iri(name);
    let has_verdict = format!("{GMEOW}hasGoalDirectedVerdict");
    let atom_pred = format!("{GMEOW}goalDirectedVerdictAtom");
    let verdict_pred = format!("{GMEOW}goalDirectedVerdict");
    objects_of(triples, &q, &has_verdict)
        .into_iter()
        .flat_map(|verdict| {
            let atoms = objects_of(triples, verdict, &atom_pred);
            let verdicts = objects_of(triples, verdict, &verdict_pred);
            atoms.into_iter().flat_map(move |atom| {
                verdicts
                    .clone()
                    .into_iter()
                    .map(move |v| (atom.to_owned(), v.to_owned()))
            })
        })
        .collect()
}

#[test]
fn shipped_bundle_goal_directed_graph_is_nonvacuous() {
    let triples = graph_triples(GRAPH);
    assert!(
        !triples.is_empty(),
        "the shipped gmeow.gts must carry a non-empty `graph/goal-directed` named graph"
    );

    // Every one of the six authored programs (slices/grounding/logic/examples/
    // reasoning-programs.ttl) is present as a projected `goalDirectedName`.
    let name_pred = format!("{GMEOW}goalDirectedName");
    let names: std::collections::BTreeSet<&str> = triples
        .iter()
        .filter(|(_, p, _)| p == &name_pred)
        .map(|(_, _, o)| o.as_str())
        .collect();
    for expected in [
        "peanoAdd",
        "memberCons",
        "winWfs",
        "mathSubsort",
        "mathSubsortControl",
        "reachability",
    ] {
        assert!(
            names.contains(expected),
            "expected program {expected:?} to be projected in graph/goal-directed, found: {names:?}"
        );
    }

    // peanoAdd: the proof-checked R = s(s(s(zero))) answer atom.
    let peano_atom =
        format!("{EX}add({EX}s({EX}s({EX}zero)),{EX}s({EX}zero),{EX}s({EX}s({EX}s({EX}zero))))");
    let peano_answers = answers_of(&triples, "peanoAdd");
    assert!(
        peano_answers.contains(&peano_atom),
        "peanoAdd: expected the answer atom {peano_atom:?}, got {peano_answers:?}"
    );

    // reachability: both reachable-pair answer atoms.
    let reach_answers = answers_of(&triples, "reachability");
    for expected in [
        format!("{EX}reach({EX}a,{EX}b)"),
        format!("{EX}reach({EX}a,{EX}c)"),
    ] {
        assert!(
            reach_answers.contains(&expected),
            "reachability: expected the answer atom {expected:?}, got {reach_answers:?}"
        );
    }

    // winWfs: the three-valued SLG-WFS verdicts (founded true/false, undefined loop).
    let win_verdicts = verdicts_of(&triples, "winWfs");
    for expected in [
        (format!("{EX}win({EX}c)"), "true".to_owned()),
        (format!("{EX}win({EX}a)"), "undefined".to_owned()),
        (format!("{EX}win({EX}d)"), "false".to_owned()),
    ] {
        assert!(
            win_verdicts.contains(&expected),
            "winWfs: expected verdict {expected:?}, got {win_verdicts:?}"
        );
    }

    // memberCons: the three M-bindings of the cons-list membership demonstrator.
    let member_bindings = bindings_of(&triples, "memberCons");
    for var in [
        format!("M = {EX}a"),
        format!("M = {EX}b"),
        format!("M = {EX}c"),
    ] {
        assert!(
            member_bindings.contains(&var),
            "memberCons: expected binding {var:?}, got {member_bindings:?}"
        );
    }

    // mathSubsort (F-4 differential): the reasoned ℤ⊑ℝ closure reached the FULL pipeline's
    // projected bundle, not merely a hardcoded tower in a unit test.
    let subsort_answers = answers_of(&triples, "mathSubsort");
    let subsort_atom = format!("{EX}p({EX}one)");
    assert!(
        subsort_answers.contains(&subsort_atom),
        "mathSubsort: expected the answer atom {subsort_atom:?}, got {subsort_answers:?}"
    );

    // mathSubsortControl (R6 presence-of-absence): status "ok" AND zero answers — a positive
    // assertion that the empty result is a real "ok, zero answers", not a silently dropped
    // program masquerading as a correct empty result.
    let control_query = query_iri("mathSubsortControl");
    let status_pred = format!("{GMEOW}goalDirectedStatus");
    let control_status = objects_of(&triples, &control_query, &status_pred);
    assert_eq!(
        control_status,
        vec!["ok"],
        "mathSubsortControl: expected status \"ok\", got {control_status:?}"
    );
    let has_answer_pred = format!("{GMEOW}hasGoalDirectedAnswer");
    let control_answer_edges = objects_of(&triples, &control_query, &has_answer_pred);
    assert!(
        control_answer_edges.is_empty(),
        "mathSubsortControl: expected ZERO hasGoalDirectedAnswer edges, got {control_answer_edges:?}"
    );

    // Every query node is typed `gmeow:GoalDirectedQuery` — the shipped graph carries the
    // structural type, not just loose literal triples.
    let query_type = format!("{GMEOW}GoalDirectedQuery");
    for name in &names {
        let q = query_iri(name);
        let types = objects_of(&triples, &q, RDF_TYPE);
        assert!(
            types.contains(&query_type.as_str()),
            "{name}: expected {q} to carry rdf:type gmeow:GoalDirectedQuery, got {types:?}"
        );
    }
}
