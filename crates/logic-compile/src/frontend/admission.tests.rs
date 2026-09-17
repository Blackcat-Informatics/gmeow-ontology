// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::{reconstruct_formula, reconstruct_formula_in_context};

const PREFIXES: &str =
    "@prefix logic: <https://blackcatinformatics.ca/logic/> . @prefix ex: <urn:example:> .";

fn source(body: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(
        format!("{PREFIXES}\n{body}").as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap()
}

#[test]
fn named_source_uses_the_shared_ir_without_unioning_other_graphs() {
    use crate::frontend::reconstruct_formula_in_named_context;

    let body = "ex:root a logic:Formula ; logic:relation ex:p ; logic:argument ex:arg . ex:arg logic:termIndex 0 ; logic:termIri ex:item .";
    let dataset = purrdf::parse_dataset(
            format!("{PREFIXES} ex:root logic:not ex:root . ex:selected {{ {body} }} ex:other {{ ex:root logic:and ex:root, ex:missing . }}").as_bytes(),
            "application/trig", None,
        ).expect("named source");
    let selected = reconstruct_formula_in_named_context(
        &dataset,
        "urn:example:selected",
        "urn:example:root",
        "urn:example:context",
    )
    .expect("selected constructor tree");
    let expected =
        reconstruct_formula_in_context(&source(body), "urn:example:root", "urn:example:context")
            .expect("same default source");
    assert_eq!(selected, expected);
    assert!(
        reconstruct_formula_in_context(&dataset, "urn:example:root", "urn:example:context")
            .is_err()
    );
    assert!(
        reconstruct_formula_in_named_context(
            &dataset,
            "urn:example:absent",
            "urn:example:root",
            "urn:example:context"
        )
        .is_err()
    );
}

#[test]
fn named_source_admission_and_missing_carriers_respect_the_selected_graph() {
    use crate::frontend::reconstruct_formula_in_named_context;

    let body = "ex:root a logic:Formula ; logic:relation ex:p ; logic:argument ex:arg .";
    let mut deep = String::new();
    for depth in 0..MAX_FORMULA_SOURCE_DEPTH {
        deep.push_str(&format!(
            "ex:n{depth} a logic:Formula ; logic:not ex:n{} .",
            depth + 1
        ));
    }
    deep.push_str(&format!("ex:n{MAX_FORMULA_SOURCE_DEPTH} a logic:Formula ; logic:relation ex:p ; logic:argument ex:arg ."));
    let dataset = purrdf::parse_dataset(
            format!("{PREFIXES} ex:arg logic:termIndex 0 ; logic:termIri ex:item . ex:selected {{ {body} }} ex:deep {{ {deep} }}").as_bytes(),
            "application/trig", None,
        ).expect("named source");
    let missing = reconstruct_formula_in_named_context(
        &dataset,
        "urn:example:selected",
        "urn:example:root",
        "urn:example:context",
    )
    .expect_err("default graph cannot supply selected argument fields");
    assert_eq!(missing.code(), crate::error::Frontend::register());
    let exhausted = reconstruct_formula_in_named_context(
        &dataset,
        "urn:example:deep",
        "urn:example:n0",
        "urn:example:context",
    )
    .expect_err("selected source admission is mandatory");
    assert_eq!(exhausted.code(), crate::error::FormulaAdmission::register());
}

#[test]
fn deep_source_is_rejected_before_recursive_construction_in_both_entry_points() {
    let mut body = String::new();
    for depth in 0..MAX_FORMULA_SOURCE_DEPTH {
        body.push_str(&format!(
            "ex:n{depth} a logic:Formula ; logic:not ex:n{} .\n",
            depth + 1
        ));
    }
    body.push_str(&format!(
        "ex:n{MAX_FORMULA_SOURCE_DEPTH} a logic:Formula ; logic:relation ex:leaf ."
    ));
    let dataset = source(&body);
    for result in [
        reconstruct_formula(&dataset, "urn:example:n0"),
        reconstruct_formula_in_context(&dataset, "urn:example:n0", "urn:example:context"),
    ] {
        let error = result.unwrap_err();
        assert_eq!(error.code(), crate::error::FormulaAdmission::register());
        assert!(error.message().contains("depth limit 128"));
        assert_eq!(
            error.inner().source_ctx.focus.as_ref().unwrap().0,
            "urn:example:n0"
        );
    }
}

#[test]
fn a_small_shared_dag_cannot_expand_into_an_exponential_tree() {
    let mut body = String::new();
    for depth in 0..18 {
        let next = depth + 1;
        for branch in ['a', 'b'] {
            body.push_str(&format!(
                "ex:{branch}{depth} a logic:Formula ; logic:and ex:a{next}, ex:b{next} .\n"
            ));
        }
    }
    body.push_str("ex:a18 a logic:Formula ; logic:relation ex:a ; logic:argument ex:arg . ex:b18 a logic:Formula ; logic:relation ex:b ; logic:argument ex:arg . ex:arg logic:termIndex 0 ; logic:termIri ex:item .");
    let dataset = source(&body);
    let error = reconstruct_formula(&dataset, "urn:example:a0").unwrap_err();
    assert_eq!(error.code(), crate::error::FormulaAdmission::register());
    assert!(error.message().contains("expanded occurrences limit 65536"));
    let (_, diagnostics) = crate::frontend::parse_logic_dataset(&dataset, None).unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "FORMULA_ADMISSION_EXHAUSTED")
    );
}

#[test]
fn function_carriers_participate_in_the_same_admission_envelope() {
    let mut body =
        String::from("ex:root a logic:Formula ; logic:relation ex:p ; logic:argument ex:c0 .\n");
    for depth in 0..MAX_FORMULA_SOURCE_DEPTH {
        body.push_str(&format!("ex:c{depth} logic:termIndex 0 ; logic:termApplication ex:f{depth} . ex:f{depth} logic:functionSymbol ex:f ; logic:argument ex:c{} .\n", depth + 1));
    }
    body.push_str(&format!(
        "ex:c{MAX_FORMULA_SOURCE_DEPTH} logic:termIndex 0 ; logic:termIri ex:end ."
    ));
    let error = reconstruct_formula(&source(&body), "urn:example:root").unwrap_err();
    assert_eq!(error.code(), crate::error::FormulaAdmission::register());
}

#[test]
fn cycles_retain_the_strict_parser_diagnostic_and_sharing_preserves_identity() {
    let cyclic = source("ex:root a logic:Formula ; logic:not ex:root .");
    let error = reconstruct_formula(&cyclic, "urn:example:root").unwrap_err();
    assert_eq!(error.code(), crate::error::Frontend::register());
    assert!(error.message().contains("recursive constructor cycle"));
    let shared = source(
        "ex:root a logic:Formula ; logic:and ex:a, ex:b . ex:a a logic:Formula ; logic:not ex:c . ex:b a logic:Formula ; logic:or ex:c, ex:d . ex:c a logic:Formula ; logic:relation ex:c ; logic:argument ex:arg . ex:d a logic:Formula ; logic:relation ex:d ; logic:argument ex:arg . ex:arg logic:termIndex 0 ; logic:termIri ex:item .",
    );
    let admitted = reconstruct_formula(&shared, "urn:example:root").unwrap();
    use crate::ir::{Formula, Term};
    let atom = |relation: &str| {
        Formula::atom(
            Term::Iri(relation.into()),
            vec![Term::Iri("urn:example:item".into())],
        )
        .unwrap()
    };
    let c = atom("urn:example:c");
    let expected = Formula::And(vec![
        Formula::Not(Box::new(c.clone())),
        Formula::Or(vec![c, atom("urn:example:d")]),
    ]);
    assert_eq!(
        admitted.content_key(),
        expected.content_key(),
        "shared nodes retain each syntactic occurrence"
    );
}

#[test]
fn annotation_only_temporal_links_have_the_same_source_admission_limit() {
    use crate::frontend::reconstruct_formula_in_named_context;

    let mut builder = purrdf::RdfDatasetBuilder::new();
    let graph = builder.intern_iri("urn:example:source");
    let next = builder.intern_iri(&logic_iri("next"));
    for depth in 0..MAX_FORMULA_SOURCE_DEPTH {
        let node = builder.intern_iri(&format!("urn:example:n{depth}"));
        let child = builder.intern_iri(&format!("urn:example:n{}", depth + 1));
        builder.push_annotation_in_graph(node, next, child, Some(graph));
    }
    let source = builder.freeze().unwrap();
    let error = reconstruct_formula_in_named_context(
        &source,
        "urn:example:source",
        "urn:example:n0",
        "urn:example:context",
    )
    .unwrap_err();
    assert_eq!(error.code(), crate::error::FormulaAdmission::register());
    assert_eq!(
        error.inner().source_ctx.focus.as_ref().unwrap().0,
        "urn:example:n0"
    );
    assert!(error.message().contains("depth limit 128"));
}
