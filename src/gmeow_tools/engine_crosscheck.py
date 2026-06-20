"""Cross-check that rdflib and gmeow_rdf answer every committed query alike.

The test suite trusts gmeow_rdf for speed (:mod:`gmeow_tools.sparql`); this gate
is what *licenses* that trust. Every committed SPARQL query under ``queries/`` is
run on the same data graph under **both** engines and their answers compared by
value (CONSTITUTION Principle 7 — verified by construction; it extends the RDF 1.2
round-trip cross-check of #177 to the whole query surface). If the engines ever
diverge, ``make check`` fails.

Comparison is **value-based**, not lexical: ``"645.0"^^xsd:decimal`` and
``"645"^^xsd:decimal`` are equal, since the two engines canonicalize numeric
literals differently while meaning the same thing.

What this does NOT cover (no oxigraph equivalent — they stay single-engine, run
full in the authoritative gate): SHACL (gmeow_shacl) and OWL reasoning
(Jena/ROBOT). Queries are run on the *asserted* merged graph: the gate proves
engine equivalence, not entailment, so reasoning is intentionally absent.
"""

from __future__ import annotations

import re
import sys
from collections import Counter
from collections.abc import Iterable
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import cast

import gmeow_rdf
from rdflib import BNode, Graph, Literal, URIRef
from rdflib.namespace import XSD
from rdflib.query import ResultRow

from gmeow_tools import sparql
from gmeow_tools.config import (
    AUDIT_QUERY_DIR,
    COMPETENCY_DIR,
    FIXTURES_DIR,
    PROJECTION_QUERY_DIR,
    QC_DIR,
    TEMPORAL_QUERY_DIR,
    VERIFY_DIR,
)
from gmeow_tools.graph import load_merged_graph, shared_merged_graph
from gmeow_tools.slices import iter_slice_query_files

#: Example instance fixtures the projection CONSTRUCTs need to produce output.
_PROJECTION_FIXTURES = (
    "places.ttl",
    "names.ttl",
    "languages.ttl",
    "identity.ttl",
    "contact-fields.ttl",
    "coreference.ttl",
    "events.ttl",
    "rights.ttl",
    "tags.ttl",
    "aggregation.ttl",
)

#: Numeric datatypes whose lexical form differs across engines but whose value
#: does not — compared as :class:`~decimal.Decimal` so ``645.0 == 645``.
_NUMERIC = frozenset(
    {
        XSD.decimal,
        XSD.integer,
        XSD.double,
        XSD.float,
        XSD.long,
        XSD.int,
        XSD.short,
        XSD.nonNegativeInteger,
        XSD.positiveInteger,
        XSD.nonPositiveInteger,
        XSD.negativeInteger,
        XSD.unsignedLong,
        XSD.unsignedInt,
    }
)

_FORM = re.compile(r"\b(SELECT|ASK|CONSTRUCT|DESCRIBE)\b", re.IGNORECASE)

#: A decimal number token inside a string literal. The two engines render
#: ``STR(?decimal)`` differently (rdflib keeps trailing zeros, gmeow_rdf
#: canonicalizes), so e.g. a constructed ``POINT(-113.924350 ...)`` WKT string
#: differs only in trailing zeros — value-equal, lexically not. Normalizing each
#: numeric token consistently on both sides makes them compare equal.
_NUM_TOKEN = re.compile(r"-?\d+\.\d+")


def _canon_numbers(text: str) -> str:
    """Canonicalize decimal-number tokens inside a string (engine-stable)."""
    return _NUM_TOKEN.sub(lambda m: str(Decimal(m.group()).normalize()), text)


@dataclass(frozen=True, slots=True)
class CrosscheckResult:
    """The outcome of cross-checking one query across both engines."""

    name: str
    form: str
    agree: bool
    detail: str = ""
    skipped: bool = False


def _query_form(text: str) -> str:
    """Return the query form (SELECT/ASK/CONSTRUCT/DESCRIBE), ignoring prefixes."""
    stripped = "\n".join(
        line
        for line in text.splitlines()
        if not line.lstrip().upper().startswith(("PREFIX", "BASE", "#"))
    )
    match = _FORM.search(stripped)
    return match.group(1).upper() if match else "SELECT"


def _term_key(term: object) -> object:
    """A value-based, engine-stable comparison key for one term."""
    if term is None:
        return None
    if isinstance(term, URIRef | BNode):
        # A URI compares by its IRI string. Blank-node *labels* are not engine-
        # stable, so blank nodes compare by kind only — sound here because the
        # committed queries return no blank nodes (and none in a row position).
        return ("uri", str(term)) if isinstance(term, URIRef) else ("bnode",)
    if isinstance(term, Literal):
        datatype = term.datatype
        if datatype in _NUMERIC:
            try:
                return ("num", Decimal(str(term)))
            except (InvalidOperation, ValueError):
                pass
        if term.language is not None:
            return ("lang", _canon_numbers(str(term)), term.language.lower())
        # A plain literal (datatype None) and an xsd:string literal mean the same
        # thing; the two engines disagree on which to emit, so fold them together.
        dt_key = (
            "str" if (datatype is None or datatype == XSD.string) else str(datatype)
        )
        return ("lit", _canon_numbers(str(term)), dt_key)
    return ("other", str(term))


def _triple_keys(graph: Graph) -> Counter[tuple[object, object, object]]:
    return Counter((_term_key(s), _term_key(p), _term_key(o)) for s, p, o in graph)


def _row_keys(
    rows: Iterable[tuple[object, ...]],
) -> Counter[tuple[object, ...]]:
    return Counter(tuple(_term_key(c) for c in row) for row in rows)


def crosscheck_query(
    name: str, query_text: str, data_graph: Graph, store: gmeow_rdf.Store
) -> CrosscheckResult:
    """Run one query on both engines over the same data and compare by value.

    Errors are caught per engine: if *both* engines reject a file (e.g. a
    demonstration file holding several queries, not one executable query) the
    check is **skipped** and reported as such — never silently dropped. If only
    one engine errors, that is a genuine divergence.
    """
    form = _query_form(query_text)
    a_ok, a_val, a_err = _run_rdflib(form, query_text, data_graph)
    b_ok, b_val, b_err = _run_pyox(form, query_text, store)

    if not a_ok and not b_ok:
        return CrosscheckResult(
            name,
            form,
            agree=True,
            detail=f"both engines rejected: {a_err}",
            skipped=True,
        )
    if a_ok != b_ok:
        return CrosscheckResult(
            name,
            form,
            agree=False,
            detail=f"rdflib_ok={a_ok}({a_err}) pyox_ok={b_ok}({b_err})",
        )
    if form == "ASK":
        agree = a_val == b_val
        return CrosscheckResult(
            name, form, agree, "" if agree else f"rdflib={a_val} pyox={b_val}"
        )
    agree = a_val == b_val
    return CrosscheckResult(name, form, agree, "" if agree else _delta(a_val, b_val))


def _run_rdflib(
    form: str, query_text: str, data_graph: Graph
) -> tuple[bool, object, str]:
    # rdflib's pyparsing-based SPARQL parser can RecursionError on very large
    # UNION chains (e.g. the schema-org projection query). The query is valid
    # SPARQL — gmeow_rdf parses it fine — so we temporarily raise the limit.
    old_limit = sys.getrecursionlimit()
    sys.setrecursionlimit(max(old_limit, 2000))
    try:
        if form == "CONSTRUCT":
            return True, _triple_keys(data_graph.query(query_text).graph or Graph()), ""
        if form == "ASK":
            return True, bool(data_graph.query(query_text)), ""
        rows = cast("Iterable[ResultRow]", data_graph.query(query_text))
        return True, _row_keys(tuple(r) for r in rows), ""
    except Exception as exc:
        return False, None, type(exc).__name__
    finally:
        sys.setrecursionlimit(old_limit)


def _run_pyox(
    form: str, query_text: str, store: gmeow_rdf.Store
) -> tuple[bool, object, str]:
    try:
        if form == "CONSTRUCT":
            return True, _triple_keys(sparql.construct(store, query_text)), ""
        if form == "ASK":
            return True, sparql.ask(store, query_text), ""
        return True, _row_keys(sparql.select(store, query_text)), ""
    except Exception as exc:
        return False, None, type(exc).__name__


def _delta(a: object, b: object) -> str:
    """A short human-readable summary of the first few set differences."""
    assert isinstance(a, Counter) and isinstance(b, Counter)
    only_a = list((a - b).elements())[:3]
    only_b = list((b - a).elements())[:3]
    return f"rdflib-only={only_a!r} pyox-only={only_b!r}"


def _projection_data() -> tuple[Graph, gmeow_rdf.Store]:
    """The merged ontology plus the projection example fixtures, both engines."""
    graph = load_merged_graph(include_imports=False)
    paths = [
        FIXTURES_DIR / f for f in _PROJECTION_FIXTURES if (FIXTURES_DIR / f).exists()
    ]
    for path in paths:
        graph.parse(path, format="turtle")
    store = sparql.store_with(*paths, include_imports=False)
    return graph, store


def crosscheck_all() -> list[CrosscheckResult]:
    """Cross-check every query under the committed query directories."""
    # Large projection CONSTRUCT queries push pyparsing past the default
    # 1000-frame limit; 3000 gives ample headroom without risk of C stack
    # overflow on 64-bit Linux (Principle 7 — engine-crosscheck gate must run).
    old_limit = sys.getrecursionlimit()
    sys.setrecursionlimit(3000)
    try:
        base_graph = shared_merged_graph(include_imports=False)
        base_store = sparql.merged_store(include_imports=False)
        proj_graph, proj_store = _projection_data()

        plan: list[tuple[Path, Graph, gmeow_rdf.Store]] = []
        for directory in (
            AUDIT_QUERY_DIR,
            COMPETENCY_DIR,
            QC_DIR,
            VERIFY_DIR,
            TEMPORAL_QUERY_DIR,
        ):
            for rq in sorted(directory.glob("*.rq")):
                plan.append((rq, base_graph, base_store))
        # Slice-owned queries (slices/*/*/queries/<kind>/, #287). TQL is
        # covered above via TEMPORAL_QUERY_DIR (which points into its slice).
        for kind in ("competency", "verify"):
            for rq in iter_slice_query_files(kind):
                plan.append((rq, base_graph, base_store))
        for rq in sorted(PROJECTION_QUERY_DIR.glob("*.rq")):
            plan.append((rq, proj_graph, proj_store))

        results: list[CrosscheckResult] = []
        for rq, graph, store in plan:
            text = rq.read_text(encoding="utf-8")
            name = f"{rq.parent.name}/{rq.name}"
            results.append(crosscheck_query(name, text, graph, store))
        return results
    finally:
        sys.setrecursionlimit(old_limit)
