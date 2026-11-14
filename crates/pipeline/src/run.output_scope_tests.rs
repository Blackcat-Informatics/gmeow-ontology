// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::node::OutputFamily;
use std::collections::BTreeSet;

fn ids(spec: &PipelineSpec) -> BTreeSet<String> {
    spec.stages.iter().map(|stage| stage.id.clone()).collect()
}

fn family(stage: &crate::loader::StageSpec) -> OutputFamily {
    default_registry()
        .get(&stage.impl_key)
        .expect("every full-spec stage binds a registered implementation")
        .output_family()
}

/// The `all` scope runs the whole build: selection must not remove any stage from it.
#[test]
fn the_all_scope_keeps_every_stage() {
    let full = full_spec();
    let all = spec_for_scope(full_spec(), RunOutputScope::All).unwrap();
    assert_eq!(ids(&all), ids(&full));
}

/// The committed scope runs exactly the stages whose outputs it selects and their
/// transitive producers: every `Generated` stage is kept, the kept set is closed under
/// `consumes`, and a stage is dropped only when it is runtime-only and no kept stage
/// consumes it. A stage with no declared family cannot exist (the trait method is
/// required), so this also covers every stage the build contains.
#[test]
fn the_committed_scope_runs_exactly_the_stages_its_outputs_select() {
    let full = full_spec();
    let committed = spec_for_scope(full_spec(), RunOutputScope::Committed).unwrap();
    let kept = ids(&committed);
    for stage in &full.stages {
        if family(stage) == OutputFamily::Generated {
            assert!(
                kept.contains(&stage.id),
                "{} is a generated stage",
                stage.id
            );
        }
    }
    for stage in committed.stages.iter() {
        for producer in &stage.consumes {
            assert!(
                kept.contains(producer),
                "{} consumes {producer}, which the scope dropped",
                stage.id
            );
        }
    }
    for stage in full.stages.iter().filter(|stage| !kept.contains(&stage.id)) {
        assert_eq!(
            family(stage),
            OutputFamily::RuntimeProjection,
            "{} was dropped from the committed scope",
            stage.id
        );
    }
    committed
        .validate()
        .expect("the committed scope keeps one sink and an acyclic graph");
}

/// The stages the committed scope no longer runs: the exporters whose only outputs are
/// gitignored `dist/` projections, consumed by no selected stage. Changing this set is a
/// change to what `make check` covers and must be a deliberate edit here.
#[test]
fn the_committed_scope_drops_only_the_runtime_projection_exporters() {
    let full = full_spec();
    let committed = spec_for_scope(full_spec(), RunOutputScope::Committed).unwrap();
    let dropped: Vec<String> = ids(&full).difference(&ids(&committed)).cloned().collect();
    assert_eq!(
        dropped,
        [
            "stage-export-export",
            "stage-export-okf",
            "stage-export-yaml-ld"
        ]
    );
}

/// A stage whose implementation is not registered has no readable output family, so
/// scope selection refuses it rather than guessing a family.
#[test]
fn a_stage_without_a_registered_implementation_is_refused() {
    let mut spec = full_spec();
    spec.stages[0].impl_key = "no-such-implementation".to_owned();
    let error = spec_for_scope(spec, RunOutputScope::Committed)
        .unwrap_err()
        .to_string();
    assert!(error.contains("no-such-implementation"), "{error}");
}
