# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The GMEOW → maximally-interlinked transpiler (#34, Phase 1).

``MAXIMAL(G) = G + E(G) + P(G)``: author the profile ONCE as canonical GMEOW;
derive the multi-vocabulary file as a compiled, auditable artifact. ``E(G)``
is the strong-equivalence saturation (:mod:`gmeow_tools.saturate`); ``P(G)``
materializes every executable projection profile
(:mod:`gmeow_tools.projections`). Both read the ORIGINAL ``G`` — projection
output is never chained back in (no fixpoint, clean provenance).

Outputs (``gmeow transform <abox>``):

* ``<stem>.gts`` — the canonical artifact: base triples + every derived
  triple's RDF 1.2 provenance reifier (``gmeow:mappedFrom`` → the authoring
  cell / projection alignment IRI, ``gmeow:confidence`` when recorded),
  emitted natively by the GTS producer (no Jena hot path).
* ``index.nq`` — the full RDF 1.2 N-Quads form (the gts→nquads shim).
* ``index.ttl`` / ``index.jsonld`` / ``index.nt`` — the ASSERTED BASE TRIPLES
  only, in three plain-RDF syntaxes: a consumer ignorant of RDF 1.2 still parses
  every triple (maximal readability); the audit trail lives in ``.gts``/``.nq``.

Gates: the alignment-direction lint authorizes saturation (ERROR rows are
refused; an ``equivalence-collapse`` ERROR aborts the whole transform — a
poisoned strong-edge graph cannot be repaired row-wise, #284). Suppression
(#282) is enforced over BOTH derivation paths. Blank nodes are skolemized
(content-addressed via the canonical labels) so reruns are diffable.
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from types import ModuleType
from typing import TYPE_CHECKING, cast

import gmeow_slice
from gmeow_rdf.compat.rdflib import Graph, URIRef

from gmeow_tools.config import DIST_DIR, PREFIXES, PROJECT_ROOT
from gmeow_tools.graph import bind_prefixes
from gmeow_tools.language_tags import filter_graph
from gmeow_tools.saturate import load_cells
from gmeow_tools.up_projection import _graph_from_native_nt
from gmeow_tools.up_projection_audit import _ontology_nt

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from gmeow_tools.language_tags import LangSelector


def _pipeline() -> ModuleType:
    from gmeow_native import pipeline

    return cast(ModuleType, pipeline)


class TransformAbortedError(RuntimeError):
    """The transform refused to run (e.g. an equivalence-collapse ERROR)."""


@dataclass(slots=True)
class TransformReport:
    """Counts and output paths of one transform run."""

    asserted: int = 0
    saturated: int = 0
    projected: int = 0
    suppressed_dropped: int = 0
    denied_cells: int = 0
    wall_clock_s: float = 0.0
    written: list[Path] = field(default_factory=list)


def _skolemized(abox: Graph) -> Graph:
    """Skolemize blank nodes through the native transform core."""
    source_nt = abox.serialize(format="nt", encoding="utf-8").decode("utf-8")
    out = _graph_from_native_nt(_pipeline().transform_skolemize_nt(source_nt))
    bind_prefixes(out)
    return out


def _denied_cells() -> set[tuple[str, str, str]]:
    """The direction lint's ERROR rows — the saturation refusal set (#25).

    On a wheel-only install (no source tree) the lint cannot run — it reads the
    SSSOM tables and the vendored target axioms — so the set precomputed at
    bundle-build time is used instead (#bundle: the CLI razor; the
    equivalence-collapse abort below is enforced at build, never reached here).

    Raises:
        TransformAbortedError: On any ``equivalence-collapse`` ERROR (#284): the
            strong-edge graph itself connects disjoint classes, and no
            per-row denial can repair a poisoned chain.
    """
    from gmeow_tools.bundle import bundled_denied_cells, repo_sources_present

    if not repo_sources_present():
        precomputed = bundled_denied_cells()
        if precomputed is not None:
            return set(precomputed)

    checks = frozenset(gmeow_slice.alignment_policy()["alignment_checks"])
    findings = [
        finding
        for finding in gmeow_slice.lint_projection(
            str(PROJECT_ROOT), allow_network=False
        )
        if finding["check"] in checks
    ]
    collapses = [
        f
        for f in findings
        if f["severity"] == "ERROR" and f["check"] == "equivalence-collapse"
    ]
    if collapses:
        details = "; ".join(f["message"] for f in collapses[:3])
        msg = f"equivalence-collapse ERROR — transform refused: {details}"
        raise TransformAbortedError(msg)
    return {
        (f["subject_id"], f["predicate_id"], f["object_id"])
        for f in findings
        if f["severity"] == "ERROR"
        and f["subject_id"] is not None
        and f["predicate_id"] is not None
        and f["object_id"] is not None
    }


def _serialize_outputs(
    base_plus_derived: Graph,
    gts_bytes: bytes,
    out_dir: Path,
    stem: str,
    *,
    selector: LangSelector | None = None,
) -> list[Path]:
    """Write .gts / index.nq / index.ttl / index.jsonld / index.nt, all verified.

    Tier discipline (#452): the canonical ``.gts`` / ``.nq`` carry the full RDF
    1.2 provenance AND the canonical internal ``x-gmeow-*`` language tags; the
    consumer tiers (``.ttl`` / ``.jsonld`` / ``.nt``) are clean asserted triples
    with public BCP-47 tags — a consumer parser (Google included) reads
    ``x-gmeow-english`` as nothing, so the readable serializations must not leak
    it. The ``.gts``/``.nq`` are written first (from the canonical bytes); then
    ``base_plus_derived`` is retagged in place for the consumer tiers, and
    finally language-filtered if a selector was requested.
    """
    import gmeow_rdf
    from gts import read, to_nquads

    from gmeow_tools.language_tags import retag_graph

    out_dir.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []

    gts_path = out_dir / f"{stem}.gts"
    gts_path.write_bytes(gts_bytes)
    written.append(gts_path)

    nq_text = to_nquads(read(gts_bytes))
    nq_path = out_dir / "index.nq"
    nq_path.write_text(nq_text, encoding="utf-8")
    # RDF 1.2 verification: the trusted parser must accept every line.
    list(gmeow_rdf.parse(nq_text.encode(), format=gmeow_rdf.RdfFormat.N_QUADS))
    written.append(nq_path)

    # consumer-facing tiers only: retag x-gmeow-* → public BCP-47 across the whole
    # graph (base + derived together; idempotent for the derived triples
    # project_graph already retagged), then apply the requested language filter.
    # The canonical .gts/.nq keep the internal tags for round-trip fidelity.
    retag_graph(base_plus_derived)
    if selector is not None:
        filter_graph(base_plus_derived, selector)

    ttl_path = out_dir / "index.ttl"
    base_plus_derived.serialize(destination=ttl_path, format="turtle")
    check = Graph()
    check.parse(ttl_path, format="turtle")
    if len(check) != len(base_plus_derived):
        msg = f"index.ttl round-trip changed triple count: {ttl_path}"
        raise ValueError(msg)
    written.append(ttl_path)

    jsonld_path = out_dir / "index.jsonld"
    base_plus_derived.serialize(
        destination=jsonld_path, format="json-ld", auto_compact=True
    )
    check = Graph()
    check.parse(jsonld_path, format="json-ld")
    if len(check) != len(base_plus_derived):
        msg = f"index.jsonld round-trip changed triple count: {jsonld_path}"
        raise ValueError(msg)
    written.append(jsonld_path)

    nt_path = out_dir / "index.nt"
    base_plus_derived.serialize(destination=nt_path, format="nt")
    check = Graph()
    check.parse(nt_path, format="nt")
    if len(check) != len(base_plus_derived):
        msg = f"index.nt round-trip changed triple count: {nt_path}"
        raise ValueError(msg)
    written.append(nt_path)
    return written


def transform(
    abox_path: Path,
    *,
    out_dir: Path | None = None,
    profiles: Sequence[str] | None = None,
    selector: LangSelector | None = None,
) -> TransformReport:
    """Run MAXIMAL(G) over an A-Box *file*.

    Args:
        abox_path: The canonical GMEOW instance Turtle file.
        out_dir: Output directory (default ``dist/transform/<stem>/``).
        profiles: Projection profiles for P(G) (default: all registered).
        selector: Optional language selector for projected/consumer labels.

    Returns:
        The :class:`TransformReport` (counts, wall clock, written paths).
    """
    raw = Graph()
    raw.parse(abox_path, format="turtle")
    return transform_graph(
        raw, abox_path.stem, out_dir=out_dir, profiles=profiles, selector=selector
    )


def transform_graph(
    raw: Graph,
    stem: str,
    *,
    out_dir: Path | None = None,
    profiles: Sequence[str] | None = None,
    selector: LangSelector | None = None,
) -> TransformReport:
    """Run MAXIMAL(G) over an in-memory A-Box graph.

    The graph-core of :func:`transform` — used directly by the end-to-end
    :func:`gmeow_tools.transpile.transpile` so an up-projected GMEOW graph flows
    into the maximal projection without a temp file.

    Args:
        raw: The canonical GMEOW A-Box graph.
        stem: The output basename (the ``.gts`` file and default sub-directory).
        out_dir: Output directory (default ``dist/transform/<stem>/``).
        profiles: Projection profiles for P(G) (default: all registered).
        selector: Optional language selector for projected/consumer labels.

    Returns:
        The :class:`TransformReport` (counts, wall clock, written paths).
    """
    from gmeow_tools.projections import PROFILES, _load_projection_query

    start = time.perf_counter()
    target = out_dir if out_dir is not None else DIST_DIR / "transform" / stem
    names = list(PROFILES) if profiles is None else list(profiles)
    unknown = sorted(set(names) - set(PROFILES))
    if unknown:
        msg = f"unknown projection profile(s): {', '.join(unknown)}"
        raise ValueError(msg)

    denied = _denied_cells()
    cells = [
        (
            str(cell.iri),
            str(cell.subject),
            cell.predicate_curie,
            str(cell.obj),
            cell.confidence,
        )
        for cell in load_cells()
    ]
    raw_nt = raw.serialize(format="nt", encoding="utf-8").decode("utf-8")
    native = _pipeline().transform_project_nt(
        raw_nt,
        _ontology_nt(),
        cells,
        sorted(denied),
        [(name, _load_projection_query(name)) for name in names],
    )

    base_plus_derived = _graph_from_native_nt(native["base_plus_derived_nt"])
    bind_prefixes(base_plus_derived)

    written = _serialize_outputs(
        base_plus_derived, bytes(native["gts_bytes"]), target, stem, selector=selector
    )

    return TransformReport(
        asserted=native["asserted"],
        saturated=native["saturated"],
        projected=native["projected"],
        suppressed_dropped=native["suppressed_dropped"],
        denied_cells=len(denied),
        wall_clock_s=time.perf_counter() - start,
        written=written,
    )


def vocab_coverage(maximal: Graph, target: Graph) -> str:
    """A vocabulary-coverage diff (the #34 backlog generator), as Markdown.

    Compares the PREDICATE + CLASS vocabulary of a MAXIMAL(G) output against
    a parity-target graph, grouped by namespace — never triple-exact (the
    instance data differs); the *missing terms* column is the backlog.
    """
    if len(maximal) == 0 or len(target) == 0:
        msg = "vocab_coverage needs two non-empty graphs"
        raise ValueError(msg)

    def vocab_terms(g: Graph) -> set[str]:
        from gmeow_rdf.compat.rdflib import RDF as _RDF

        terms = {str(p) for p in g.predicates()}
        terms.update(
            str(o) for o in g.objects(None, _RDF.type) if isinstance(o, URIRef)
        )
        return terms

    def by_namespace(terms: set[str]) -> dict[str, set[str]]:
        sorted_prefixes = sorted(PREFIXES.items(), key=lambda kv: -len(kv[1]))
        grouped: dict[str, set[str]] = {}
        for term in terms:
            for prefix, ns in sorted_prefixes:
                if term.startswith(ns):
                    grouped.setdefault(prefix, set()).add(term[len(ns) :])
                    break
            else:
                ns = term.rsplit("#", 1)[0] if "#" in term else term.rsplit("/", 1)[0]
                grouped.setdefault(ns, set()).add(term.rsplit("/", 1)[-1])
        return grouped

    ours = by_namespace(vocab_terms(maximal))
    theirs = by_namespace(vocab_terms(target))

    lines = [
        "| vocabulary | terms in target | covered | missing |",
        "|---|---|---|---|",
    ]
    total_target = total_covered = 0
    for vocab_name in sorted(theirs):
        target_terms = theirs[vocab_name]
        covered = target_terms & ours.get(vocab_name, set())
        missing = sorted(target_terms - covered)
        total_target += len(target_terms)
        total_covered += len(covered)
        shown = ", ".join(f"`{m}`" for m in missing[:8])
        if len(missing) > 8:
            shown += f" … +{len(missing) - 8} more"
        lines.append(
            f"| {vocab_name} | {len(target_terms)} | {len(covered)} | {shown or '—'} |"
        )
    lines.append(f"| **total** | **{total_target}** | **{total_covered}** | |")
    return "\n".join(lines) + "\n"
