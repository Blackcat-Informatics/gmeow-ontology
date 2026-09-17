// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! External metadata envelopes participate before native consumer refinement.

use super::*;

#[test]
fn abstract_contextual_outputs_activate_only_their_owner_and_declared_markers() {
    let marker = "https://blackcatinformatics.ca/logic/ReasoningResult";
    let type_predicate = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let information = "https://blackcatinformatics.ca/logic/resultInformation";
    let consumer = |name: &str, predicate: &str, object: &str| {
        EvalRule::positive(
            name,
            EvalAtom::positive(EvalTerm::var("subject"), name, EvalTerm::named("urn:ready")),
            vec![EvalAtom::positive(
                EvalTerm::var("subject"),
                predicate,
                EvalTerm::named(object),
            )],
        )
    };
    let rules = [
        consumer("urn:result-consumer", type_predicate, marker),
        consumer(
            "urn:foreign-marker-consumer",
            type_predicate,
            "urn:ForeignMarker",
        ),
        consumer("urn:known-value-consumer", information, "urn:KnownValue"),
    ];
    let template =
        Arc::new(JointTemplate::new(&rules, &[], &[], SemanticVocabulary::Exact).unwrap());
    let facts = BTreeMap::from([
        ("urn:owner".to_owned(), Vec::new()),
        ("urn:other".to_owned(), Vec::new()),
    ]);
    let output = |owner: &str| WorldProducerEffect {
        owner: owner.into(),
        effect: ProducerEffect::new(
            "selected-contextual-assessment".into(),
            vec![
                StatementPattern::relation(Some(type_predicate), Some(marker)),
                StatementPattern::relation(Some(information), None),
            ],
            Vec::new(),
        ),
        read_worlds: Vec::new(),
    };
    let absent = template.input(&facts, Arc::from([]), &[]).unwrap();
    let selected = template
        .input(&facts, Arc::from([]), &[output("urn:owner")])
        .unwrap();
    assert!(
        absent
            .world_effects
            .values()
            .all(|effects| effects.iter().all(|effect| effect.writes.is_empty()))
    );
    let owner = &selected.world_effects["urn:owner"];
    assert!(
        !owner[0].writes.is_empty(),
        "a possible metadata class must enable its real consumer"
    );
    assert!(
        owner[1].writes.is_empty(),
        "the exact type marker must survive abstraction"
    );
    assert!(
        !owner[2].writes.is_empty(),
        "an unknown object includes existing program constants"
    );
    assert!(
        selected.world_effects["urn:other"]
            .iter()
            .all(|effect| effect.writes.is_empty())
    );
    assert_ne!(absent.identity(), selected.identity());
    let moved = template
        .input(&facts, Arc::from([]), &[output("urn:other")])
        .unwrap();
    assert_ne!(
        selected.identity(),
        moved.identity(),
        "the exact external owner belongs to admission identity"
    );
    assert!(
        moved.world_effects["urn:owner"]
            .iter()
            .all(|effect| effect.writes.is_empty())
    );
    assert!(
        selected.evidence.is_none(),
        "concrete-only source termination evidence cannot cover unknown metadata outputs"
    );
    assert!(
        template
            .input(&facts, Arc::from([]), &[output("urn:missing")])
            .is_err()
    );
}
