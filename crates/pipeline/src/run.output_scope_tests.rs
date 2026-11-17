// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::node::OutputFamily;
use crate::registry::{default_registry, registry_for};
use std::collections::BTreeSet;

fn ids(spec: &PipelineSpec) -> BTreeSet<String> {
    spec.stages.iter().map(|stage| stage.id.clone()).collect()
}

/// The `all` scope runs the whole build: selection must not remove any stage from it.
#[test]
fn the_all_scope_keeps_every_stage() {
    let full = full_spec();
    for profile in [BuildProfile::Ordinary, BuildProfile::Release] {
        let registry = registry_for(profile);
        let all = spec_for_scope(full_spec(), RunOutputScope::All, &registry)
            .expect("every declared family agrees with the selected build profile");
        assert_eq!(ids(&all), ids(&full));
    }
}

/// The committed scope runs exactly the stages whose outputs it selects and their
/// transitive producers: every `Generated` stage is kept, the kept set is closed under
/// `consumes`, and a stage is dropped only when it is runtime-only and no kept stage
/// consumes it. Admission checks every authored family against the selected profile.
#[test]
fn the_committed_scope_runs_exactly_the_stages_its_outputs_select() {
    let full = full_spec();
    let committed =
        spec_for_scope(full_spec(), RunOutputScope::Committed, &default_registry()).unwrap();
    let kept = ids(&committed);
    for stage in &full.stages {
        if stage.output_family == OutputFamily::Generated {
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
            stage.output_family,
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
    let committed =
        spec_for_scope(full_spec(), RunOutputScope::Committed, &default_registry()).unwrap();
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
    for scope in [RunOutputScope::All, RunOutputScope::Committed] {
        let mut spec = full_spec();
        spec.stages[0].impl_key = "no-such-implementation".to_owned();
        let error = spec_for_scope(spec, scope, &default_registry())
            .expect_err("scope admission must resolve even an omitted implementation")
            .to_string();
        assert!(error.contains("no-such-implementation"), "{error}");
    }
}

/// A false runtime-family declaration cannot hide a Generated stage before bind;
/// the reverse lie is invalid too. Full binding and both scopes share admission.
#[test]
fn divergent_families_and_stage_identities_fail_before_scope_filtering() {
    let registry = default_registry();
    for family in [OutputFamily::Generated, OutputFamily::RuntimeProjection] {
        let mut spec = full_spec();
        let stage = spec
            .stages
            .iter_mut()
            .find(|stage| stage.output_family == family)
            .expect("the production inventory contains both output families");
        let id = stage.id.clone();
        stage.output_family = match family {
            OutputFamily::Generated => OutputFamily::RuntimeProjection,
            OutputFamily::RuntimeProjection => OutputFamily::Generated,
        };
        for scope in [RunOutputScope::All, RunOutputScope::Committed] {
            let error = spec_for_scope(spec.clone(), scope, &registry)
                .expect_err("family mismatch must fail before any stage disappears")
                .to_string();
            assert!(error.contains(&id), "{error}");
            assert!(error.contains("stageOutputFamily"), "{error}");
        }
        let graph = spec.validate().expect("only the family was changed");
        let error = bind(&spec, &graph, &registry)
            .err()
            .expect("direct manifest/fixture binding also checks family")
            .to_string();
        assert!(error.contains("stageOutputFamily"), "{error}");
    }
    for scope in [RunOutputScope::All, RunOutputScope::Committed] {
        let mut spec = full_spec();
        spec.stages[0].id = "wrong-stage-identity".to_owned();
        let error = spec_for_scope(spec, scope, &registry)
            .expect_err("a matching family cannot authorize a different stage's body")
            .to_string();
        assert!(error.contains("wrong-stage-identity"), "{error}");
        assert!(error.contains("whose own id"), "{error}");
    }
}
