# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Benchmark the Rust GTS producer against the legacy Python producer (#645).

Runs both the production ``["gzip"]`` path and the ``["identity"]`` path for
each producer over the same full-snapshot inputs. The identity comparison
isolates producer logic from cross-library gzip encoding differences, while
the gzip comparison mirrors the committed artifact. Results are printed and
written to ``dist/gts-producer-benchmark.json``.
"""

from __future__ import annotations

import json
import time
from collections.abc import Callable
from typing import Any

from gmeow_tools.config import (
    GTS_GRAPH_IMPORTS,
    GTS_GRAPH_METADATA,
    PROJECT_ROOT,
    SLICES_DIR,
    STATEMENT_RDF12_FILE,
)
from gmeow_tools.graph import iter_source_files, load_merged_graph
from gmeow_tools.gts_gen import (
    _doc_blobs,
    _imports_graph,
    _metadata_graph,
    _ontology_doc_blobs,
    _project_doc_blobs,
    _transform_blobs,
    build_snapshot_bytes,
)
from gmeow_tools.gts_producer import _compile_gts_legacy, compile_gts
from gmeow_tools.i18n_catalog import merge_terms
from gmeow_tools.mappings import build_alignment_graph, load_mappings
from gmeow_tools.validate import guide_anchor_lint

RUST_ITERATIONS = 3


def _build_inputs() -> tuple[object, list[tuple[bytes, str, str]]]:
    """Return the same (multilingual_graph, doc_blobs) tuple used by the generator."""
    graph = load_merged_graph(include_imports=False)
    lint = guide_anchor_lint([str(p) for p in iter_source_files(include_imports=False)])
    if lint.errors:
        details = "; ".join(lint.errors[:5])
        raise ValueError(
            f"docs-in-sync invariant violated (#325): {len(lint.errors)} "
            f"guide anchor error(s) — {details}"
        )

    po_paths = sorted(p for p in SLICES_DIR.glob("*/*/i18n/*.po") if p.stem != "en")
    multilingual_graph = merge_terms(graph, po_paths)

    blobs = (
        _doc_blobs(multilingual_graph)
        + _project_doc_blobs()
        + _ontology_doc_blobs()
        + _transform_blobs(multilingual_graph)
    )
    return multilingual_graph, blobs


def _producer_kwargs(
    multilingual_graph: object,
    blobs: list[tuple[bytes, str, str]],
    transform: list[str],
) -> dict[str, Any]:
    """Keyword arguments shared by both producer entry points."""
    return {
        "graph": multilingual_graph,
        "rdf12_path": STATEMENT_RDF12_FILE,
        "alignment_graph": build_alignment_graph(load_mappings()),
        "extra_named_graphs": [
            (_imports_graph(), GTS_GRAPH_IMPORTS, "imports"),
            (_metadata_graph(), GTS_GRAPH_METADATA, "metadata"),
        ],
        "doc_blobs": blobs,
        "transform": transform,
    }


def _time(fn: Callable[[], object], iterations: int = 1) -> tuple[float, list[float]]:
    """Return total and per-iteration wall times for *fn*."""
    times: list[float] = []
    for _ in range(iterations):
        start = time.perf_counter()
        fn()
        elapsed = time.perf_counter() - start
        times.append(elapsed)
    return sum(times), times


def _benchmark_suite(
    multilingual_graph: object,
    blobs: list[tuple[bytes, str, str]],
    transform: list[str],
) -> dict[str, Any]:
    """Run one transform comparison and return structured results."""
    kwargs = _producer_kwargs(multilingual_graph, blobs, transform)
    rust_fn = lambda: compile_gts(**kwargs)  # noqa: E731
    legacy_fn = lambda: _compile_gts_legacy(**kwargs)  # noqa: E731

    rust_total, rust_times = _time(rust_fn, RUST_ITERATIONS)
    rust_mean = rust_total / RUST_ITERATIONS
    legacy_total, _ = _time(legacy_fn, iterations=1)
    speedup = legacy_total / rust_mean if rust_mean > 0 else float("inf")

    return {
        "transform": transform[0],
        "rust_iterations": RUST_ITERATIONS,
        "rust_times_seconds": rust_times,
        "rust_mean_seconds": rust_mean,
        "rust_total_seconds": rust_total,
        "legacy_time_seconds": legacy_total,
        "speedup_factor": speedup,
    }


def _print_table(result: dict[str, Any]) -> None:
    """Print a concise timing table for one transform."""
    transform = result["transform"]
    print(f"\nTransform: {transform}")
    print("-" * 55)
    print(f"{'Implementation':<25} {'Duration (s)':>14} {'Note':<16}")
    print("-" * 55)
    for i, t in enumerate(result["rust_times_seconds"], start=1):
        print(
            f"{'Rust producer run ' + str(i):<25} {t:>14.3f} "
            f"{transform + ' transform':<16}"
        )
    print(
        f"{'Rust producer mean':<25} {result['rust_mean_seconds']:>14.3f} "
        f"{transform + ' transform':<16}"
    )
    print(
        f"{'Legacy Python producer':<25} {result['legacy_time_seconds']:>14.3f} "
        f"{transform + ' transform':<16}"
    )
    print("-" * 55)
    print(f"Speedup (legacy / rust mean): {result['speedup_factor']:.2f}x")


def main() -> None:
    """Run the benchmark and write results to ``dist/gts-producer-benchmark.json``."""
    multilingual_graph, blobs = _build_inputs()

    print("Warming Rust producer (gzip) ...")
    build_snapshot_bytes()  # warm caches / avoid first-run overhead

    results = []
    for transform in (["identity"], ["gzip"]):
        result = _benchmark_suite(multilingual_graph, blobs, transform)
        _print_table(result)
        results.append(result)

    out = PROJECT_ROOT / "dist" / "gts-producer-benchmark.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(results, indent=2) + "\n")
    print(f"\nWrote benchmark results to {out}")


if __name__ == "__main__":
    main()
