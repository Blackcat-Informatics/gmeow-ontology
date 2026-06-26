# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The universal aboutness vocabulary (#349, EPIC #348).

The mention/use distinction (describes vs enacts) is the fourth domain-free
epistemic axis alongside granularity, determinacy, and sensitivity.

The kernel-local structural invariants (AboutnessMode class structure,
hasAboutness annotation-property shape, the closed two-seed value vocabulary)
and the aboutness-modes competency question now live as slice-resident
declarative test cells in slices/core/kernel/tests/{structural,competency}.ttl,
driven by the native Rust harness (crates/slicetest, #784). See issue #867 and
dsl/tests/MIGRATION-LEDGER.md.

The two tests RETAINED here assert ABSENCE over the whole merged graph
(include_imports=False) — orthogonality across axes (gmeow:confidence,
gmeow:hasGranularity, … declared in 10+ slices) and the seeds' exactly-one-type
guarantee — which the module-scoped (gmeow:scopeModule) cell DSL cannot express
faithfully, so they stay as Python merged-graph assertions.
"""

from __future__ import annotations

from itertools import combinations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_aboutness_orthogonal_to_other_axes() -> None:
    """hasAboutness ⟂ every other kernel axis: no inferential bridge (Principle 9).

    Granularity is resolution, determinacy is ontic, sensitivity is privacy,
    confidence is epistemic, standpointModality is doxastic — aboutness is
    rhetorical. None may subsume or equate another. RETAINED in Python: the axes
    are declared across many slice modules, so this absence must be checked over
    the merged graph, not a single module (cannot reduce to gmeow:scopeModule).
    """
    g = _graph()
    axes = [
        GM.hasAboutness,
        GM.hasGranularity,
        GM.hasDeterminacy,
        GM.hasSensitivity,
        GM.hasDisclosurePolicy,
        GM.confidence,
    ]
    for a, b in combinations(axes, 2):
        assert (a, RDFS.subPropertyOf, b) not in g
        assert (b, RDFS.subPropertyOf, a) not in g
        assert (a, OWL.equivalentProperty, b) not in g
        assert (b, OWL.equivalentProperty, a) not in g


def test_no_aboutness_truth_bridge() -> None:
    """Enactment never implies assertion: no axiom links aboutness to
    veridicality or standpoint modality (the licensed-falsehood boundary is a
    documented bridge, not an entailment). RETAINED in Python: gmeow:aboutnessEnacts
    is referenced from sibling slices (citations, norms), so the exactly-one-type
    guarantee must hold over the merged graph, not just the kernel module.
    """
    g = _graph()
    for seed in (GM.aboutnessDescribes, GM.aboutnessEnacts):
        # Seeds are plain vocabulary individuals — exactly one class membership.
        types = set(g.objects(seed, RDF.type))
        assert types == {GM.AboutnessMode}
