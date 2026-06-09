"""Phase-0 benchmark for the pyoxigraph test-speed work (#242).

Measures the operations that dominate non-Docker test runtime so the speedups
claimed by this change are grounded in numbers, not vibes. Run with:

    uv run python scripts/bench_engines.py

It is a manual diagnostic, not a test (no asserts, no CI gate).
"""

from __future__ import annotations

import time
from collections.abc import Callable

import rdflib

from gmeow_tools import sparql
from gmeow_tools.config import FIXTURES_DIR, PROJECTION_QUERY_DIR, SHAPES_FILE
from gmeow_tools.graph import _build_merged_graph, load_merged_graph


def _time(label: str, fn: Callable[[], object], n: int = 5) -> None:
    fn()  # warm
    start = time.perf_counter()
    for _ in range(n):
        fn()
    ms = (time.perf_counter() - start) / n * 1000
    print(f"  {label:<46} {ms:7.1f} ms")


def main() -> None:
    """Print the benchmark table for the graph-loading and query hot paths."""
    _build_merged_graph.cache_clear()
    start = time.perf_counter()
    cached = _build_merged_graph(False)
    print(
        f"cold parse merged ontology (rdflib, 44 files): "
        f"{(time.perf_counter() - start) * 1000:.0f} ms, {len(cached)} triples\n"
    )

    print("graph loading (per call):")
    _time(
        "(a) load_merged_graph (rdflib deep copy)",
        lambda: load_merged_graph(include_imports=False),
    )
    _time("(b) merged_store (pyoxigraph, cached)", lambda: sparql.merged_store())
    _time("(c) store_with (fresh pyoxigraph store)", lambda: sparql.store_with())

    fixtures = [
        FIXTURES_DIR / f
        for f in ("places.ttl", "names.ttl", "languages.ttl", "identity.ttl")
    ]
    src = load_merged_graph(include_imports=False)
    for path in fixtures:
        src.parse(path, format="turtle")
    store = sparql.store_with(*fixtures)
    query = (PROJECTION_QUERY_DIR / "schema-org.rq").read_text(encoding="utf-8")

    print("\nschema-org CONSTRUCT projection:")
    _time("(d) rdflib .query()", lambda: src.query(query).graph, n=3)
    _time(
        "(e) pyoxigraph + hand-off to rdflib",
        lambda: sparql.construct(store, query),
        n=3,
    )

    print("\nSHACL shapes parse (per run_shacl call before caching):")
    _time(
        "(f) Graph().parse(shapes)",
        lambda: rdflib.Graph().parse(SHAPES_FILE, format="turtle"),
    )


if __name__ == "__main__":
    main()
