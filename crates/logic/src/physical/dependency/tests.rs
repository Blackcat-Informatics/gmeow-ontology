// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native scheduling contracts, independently checked against their inequalities.

use super::*;

#[test]
fn positive_recursion_and_completed_reads_have_distinct_boundaries() {
    let edges = [
        ("urn:a", "urn:b", ReadDependency::Positive),
        ("urn:b", "urn:a", ReadDependency::Positive),
        ("urn:c", "urn:a", ReadDependency::Completed),
        ("urn:d", "urn:c", ReadDependency::Positive),
        ("urn:e", "urn:d", ReadDependency::Completed),
    ];
    let plan = |reverse| {
        let mut graph = SignedDependencies::default();
        graph.define("urn:edb");
        for index in 0..edges.len() {
            let (head, read, dependency) = edges[if reverse {
                edges.len() - index - 1
            } else {
                index
            }];
            graph.read(head, read, dependency);
        }
        graph.plan().unwrap()
    };
    let forward = plan(false);
    let reverse = plan(true);
    assert_eq!(forward.component("urn:a"), forward.component("urn:b"));
    assert_ne!(forward.component("urn:c"), forward.component("urn:d"));
    for (predicate, stratum) in [
        ("urn:a", 0),
        ("urn:b", 0),
        ("urn:c", 1),
        ("urn:d", 1),
        ("urn:e", 2),
        ("urn:edb", 0),
    ] {
        assert_eq!(forward.stratum(predicate), Some(stratum));
        assert_eq!(reverse.stratum(predicate), Some(stratum));
        assert_eq!(forward.component(predicate), reverse.component(predicate));
    }
    assert_eq!(forward.stratum("urn:unregistered"), None);
}

#[test]
fn completed_cycle_names_the_strict_read_and_its_component() {
    let mut graph = SignedDependencies::default();
    graph.read("urn:a", "urn:b", ReadDependency::Positive);
    graph.read("urn:b", "urn:c", ReadDependency::Positive);
    graph.read("urn:c", "urn:a", ReadDependency::Completed);
    graph.read("urn:outside", "urn:c", ReadDependency::Positive);
    assert_eq!(
        graph.plan().unwrap_err(),
        DependencyCycle {
            members: vec!["urn:a".to_owned(), "urn:b".to_owned(), "urn:c".to_owned()],
            head: "urn:c".to_owned(),
            read: "urn:a".to_owned(),
        }
    );
}

/// Independent least-solution oracle for the signed inequalities. This deliberately
/// performs repeated relaxation rather than sharing the SCC implementation.
fn inequality_solution(edges: &[(usize, usize, ReadDependency)]) -> Option<[usize; 3]> {
    let mut values = [0usize; 3];
    for _ in 0..=3 {
        let mut changed = false;
        for &(head, read, dependency) in edges {
            let required = values[read] + usize::from(dependency == ReadDependency::Completed);
            if values[head] < required {
                values[head] = required;
                changed = true;
            }
        }
        if !changed {
            return Some(values);
        }
    }
    None
}

#[test]
fn every_three_predicate_signed_graph_matches_the_least_inequality_solution() {
    let names = ["urn:a", "urn:b", "urn:c"];
    for mut encoding in 0u32..3u32.pow(9) {
        let mut graph = SignedDependencies::default();
        for name in names {
            graph.define(name);
        }
        let mut edges = Vec::new();
        for head in 0..3 {
            for read in 0..3 {
                let kind = encoding % 3;
                encoding /= 3;
                if kind != 0 {
                    let dependency = if kind == 1 {
                        ReadDependency::Positive
                    } else {
                        ReadDependency::Completed
                    };
                    graph.read(names[head], names[read], dependency);
                    edges.push((head, read, dependency));
                }
            }
        }
        let expected = inequality_solution(&edges);
        let actual = graph.plan();
        assert_eq!(actual.is_ok(), expected.is_some(), "{edges:?}");
        if let Some(expected) = expected {
            let actual = actual.unwrap();
            for (index, name) in names.iter().enumerate() {
                assert_eq!(actual.stratum(name), Some(expected[index]), "{edges:?}");
            }
        }
    }
}

#[test]
fn a_long_reversed_producer_chain_is_stack_safe_and_exact() {
    let mut graph = SignedDependencies::default();
    for index in (1..4096).rev() {
        graph.read(
            &format!("urn:p:{index}"),
            &format!("urn:p:{}", index - 1),
            ReadDependency::Completed,
        );
    }
    let plan = graph.plan().unwrap();
    assert_eq!(plan.stratum("urn:p:0"), Some(0));
    assert_eq!(plan.stratum("urn:p:4095"), Some(4095));
}
