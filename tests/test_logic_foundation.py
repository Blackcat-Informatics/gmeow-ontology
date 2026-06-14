# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

"""GMEOW Logic foundation surface (issue #498, Task 1).

Covers the term-surface contract for the canonical ``logic:`` vocabulary:
the namespace is registered in the unified prefix registry, every minted
``logic:`` term carries an ``@x-gmeow-english`` label, a ``skos:definition``,
and the slice's ``rdfs:isDefinedBy`` IRI, and no local name carries a
Principle-9 selector token. The term list is enumerated from the module, never
hardcoded, so the test grows with the vocabulary.
"""

from __future__ import annotations

from rdflib import Graph, Literal, URIRef
from rdflib.namespace import RDFS, SKOS

from gmeow_tools.config import LOGIC_NAMESPACE, PREFIXES, SLICES_DIR

_LOGIC_MODULE = SLICES_DIR / "core" / "logic" / "module.ttl"
_LOGIC_SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/logic")
_X_GMEOW_ENGLISH = "x-gmeow-english"
#: Principle 9 forbids selector tokens in local names.
_SELECTOR_TOKENS = ("primary", "preferred", "default", "main")


def test_logic_prefix_registered() -> None:
    assert "logic" in PREFIXES
    assert PREFIXES["logic"] == LOGIC_NAMESPACE


def _logic_subjects(graph: Graph) -> set[URIRef]:
    return {
        s
        for s in set(graph.subjects())
        if isinstance(s, URIRef) and str(s).startswith(LOGIC_NAMESPACE)
    }


def test_logic_module_terms_are_complete() -> None:
    graph = Graph()
    graph.parse(_LOGIC_MODULE, format="turtle")
    subjects = _logic_subjects(graph)
    assert subjects, "the logic module must mint logic: terms"

    for subject in subjects:
        labels = list(graph.objects(subject, RDFS.label))
        assert labels, f"{subject} is missing an rdfs:label"
        assert any(
            isinstance(label, Literal) and label.language == _X_GMEOW_ENGLISH
            for label in labels
        ), f"{subject} has no @{_X_GMEOW_ENGLISH} rdfs:label"

        definitions = list(graph.objects(subject, SKOS.definition))
        assert definitions, f"{subject} is missing a skos:definition"
        assert any(
            isinstance(defn, Literal) and defn.language == _X_GMEOW_ENGLISH
            for defn in definitions
        ), f"{subject} has no @{_X_GMEOW_ENGLISH} skos:definition"

        defined_by = list(graph.objects(subject, RDFS.isDefinedBy))
        assert defined_by == [_LOGIC_SLICE_IRI], (
            f"{subject} must declare rdfs:isDefinedBy <{_LOGIC_SLICE_IRI}>, "
            f"got {defined_by}"
        )


def test_logic_local_names_have_no_selector_token() -> None:
    graph = Graph()
    graph.parse(_LOGIC_MODULE, format="turtle")
    for subject in _logic_subjects(graph):
        local = str(subject)[len(LOGIC_NAMESPACE) :].lower()
        for token in _SELECTOR_TOKENS:
            assert token not in local, (
                f"Principle 9: logic: local name {local!r} contains "
                f"selector token {token!r}"
            )
