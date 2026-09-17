// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Actual native multi-world commit and budget-cut witness publication.

use super::*;

#[test]
fn two_world_round_preserves_each_committed_introduction_and_excludes_the_cut_suffix() {
    let rule = producer(
        vec![atom("?x", "urn:seed", "?y")],
        vec![atom("?x", "urn:witness", "?n")],
    );
    let program = prepare(&[], &[rule]);
    let worlds = ["urn:world:a", "urn:world:b"];
    let complete = run(&program, &worlds, None);
    assert_eq!(complete.witness_derivations.len(), 2);
    assert_eq!(
        complete
            .witness_derivations
            .iter()
            .map(|witness| witness.scope.world.as_str())
            .collect::<BTreeSet<_>>(),
        worlds.into_iter().collect(),
    );
    for witness in &complete.witness_derivations {
        witness.validate().unwrap();
    }
    let partial = run(&program, &worlds, Some(1));
    assert_eq!(partial.result.status, BudgetStatus::Exhausted);
    assert_eq!(partial.result.consumed_steps, 1);
    assert_eq!(partial.witness_derivations.len(), 1);
    assert_eq!(partial.witness_derivations[0].scope.world, worlds[0]);
    assert!(
        !partial
            .result
            .rows
            .iter()
            .any(|row| row.graph == worlds[1] && row.predicate == "urn:witness")
    );
    partial.witness_derivations[0].validate().unwrap();
    let zero = run(&program, &worlds, Some(0));
    assert!(zero.witness_derivations.is_empty());
}
