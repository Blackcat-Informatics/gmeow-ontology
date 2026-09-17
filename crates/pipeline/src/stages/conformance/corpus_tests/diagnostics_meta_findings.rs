// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated observations preserve authored diagnostic meta-rule assertions.
use crate::stages::meta_findings::MetaDerivation;
use gmeow_ns::{GMEOW_NS, LOGIC_NS};
fn gmeow(local: &str) -> String {
    format!("{GMEOW_NS}{local}")
}
fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}
fn observed(case: &str) -> &'static MetaDerivation {
    super::super::diagnostic_observations()
        .meta
        .get(case)
        .expect("selected diagnostic case")
        .as_ref()
        .expect("native meta-rule execution succeeded")
}

#[test]
fn root_cause_and_cluster_derive_over_a_shared_root() {
    let iris = &super::super::diagnostic_observations().meta_sources;
    // The three principal + helper + extensibility rules are all discovered by TYPE.
    assert!(
        iris.contains(&logic("ruleFindingRootCause")),
        "the root-cause rule must be discovered by gmeow:DiagnosticMetaRule type"
    );

    let result = observed("conformance-fixtures/finding-root-cause-present.ttl");

    let (f1, f2, f3, f4) = (
        gmeow("examples/diagnostics/tests/rcF1"),
        gmeow("examples/diagnostics/tests/rcF2"),
        gmeow("examples/diagnostics/tests/rcF3"),
        gmeow("examples/diagnostics/tests/rcF4"),
    );

    // findingRootCause: F2, F3, F4 all point at the shared childless root F1.
    let root_cause = &result.root_cause;
    for f in [&f2, &f3, &f4] {
        assert!(
            root_cause.contains(&(f.clone(), f1.clone())),
            "expected gmeow:findingRootCause({f}, {f1}); derived: {root_cause:?}"
        );
    }
    // The intermediate F2 is NOT a root of F4 (F2 has an antecedent): honest shared root.
    assert!(
        !root_cause.contains(&(f4.clone(), f2.clone())),
        "F2 has an antecedent, so it must never be F4's root cause"
    );

    // findingCluster + clusterRoot: the group keyed by F1 gathers F2, F3, F4; the
    // cluster node (F1) carries clusterRoot to itself and is typed gmeow:FindingCluster.
    let cluster = &result.cluster;
    for f in [&f2, &f3, &f4] {
        assert!(
            cluster.contains(&(f.clone(), f1.clone())),
            "expected gmeow:findingCluster({f}, {f1}); derived: {cluster:?}"
        );
    }
    let cluster_root = &result.cluster_root_edges;
    assert!(
        cluster_root.contains(&(f1.clone(), f1.clone())),
        "expected gmeow:clusterRoot({f1}, {f1}); derived: {cluster_root:?}"
    );
    assert!(
        result.cluster_typed.contains(&f1),
        "the root-keyed cluster node F1 must be typed gmeow:FindingCluster"
    );

    // Extensibility: the THIRD tagged rule (ruleRootFinding) fires through the SAME
    // class selection — F1 is typed gmeow:RootFinding — with no engine change.
    assert!(
        iris.contains(&logic("ruleRootFinding")),
        "the extensibility rule must be discovered by gmeow:DiagnosticMetaRule type"
    );
    assert!(
        result.root_finding_typed.contains(&f1),
        "the extensibility rule must type the witnessed root F1 as gmeow:RootFinding"
    );
}

#[test]
fn independent_findings_derive_no_root_cause() {
    let result = observed("counter-examples/finding-root-cause-absent.ttl");

    assert!(
        result.root_cause.is_empty(),
        "independent findings (no antecedent chain) must derive NO gmeow:findingRootCause"
    );
    assert!(
        result.traces.is_empty(),
        "independent findings must derive NO gmeow:findingTraces"
    );
    assert!(
        result.cluster.is_empty(),
        "independent findings must derive NO gmeow:findingCluster"
    );
}

#[test]
fn cross_node_glut_derives_at_a_non_trivial_anchor() {
    let iris = &super::super::diagnostic_observations().meta_sources;
    assert!(
        iris.contains(&logic("ruleCrossNodeGlut")),
        "the cross-node-glut rule must be discovered by gmeow:DiagnosticMetaRule type"
    );

    let result = observed("conformance-fixtures/cross-node-glut-present.ttl");

    let supported = gmeow("examples/diagnostics/tests/glutSupported");
    let opposed = gmeow("examples/diagnostics/tests/glutOpposed");
    let glut = &result.glut;
    assert!(
        glut.contains(&(supported.clone(), opposed.clone())),
        "expected gmeow:crossNodeGlutWith({supported}, {opposed}); derived: {glut:?}"
    );
}

#[test]
fn cross_node_glut_never_fires_on_the_counter_examples() {
    for fixture_rel in [
        "counter-examples/cross-node-glut-same-polarity.ttl",
        "counter-examples/cross-node-glut-different-anchor.ttl",
        "counter-examples/cross-node-glut-trivial-anchor.ttl",
    ] {
        let result = observed(fixture_rel);
        assert!(
            result.glut.is_empty(),
            "the cross-node-glut rule must derive NOTHING over {fixture_rel}"
        );
    }
}
