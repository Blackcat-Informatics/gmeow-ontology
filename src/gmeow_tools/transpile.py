# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The full transpile — consumer RDF → pure GMEOW → MAXIMAL multi-vocab (#448).

The two halves, chained end to end:

1. **Up-projection** (the front half, #451): lift a non-GMEOW source graph up
   into pure GMEOW — facts for the mechanically-invertible terms, provenance-
   stamped claims for the inferred ones — resolving each edge by its position in
   the graph (the context-aware descent) over the per-term floor.
2. **Maximal down-projection** (the back half, #34): run ``MAXIMAL(G) = G + E(G)
   + P(G)`` over that pure-GMEOW draft — the canonical base, its strong-
   equivalence saturation, and every projection profile — into one fat,
   provenance-audited multi-vocabulary file family.

So a schema.org (or FOAF, vCard, …) source is ingested, understood *as GMEOW*,
and re-expressed maximally across every vocabulary GMEOW can reach — with the
pure-GMEOW intermediate written alongside as the auditable draft. The output is a
**publication**: suppressed nodes are withheld, and every derived triple carries
its ``gmeow:mappedFrom`` provenance in the ``.gts``/``.nq`` forms.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from rdflib import RDF, Graph, URIRef

from gmeow_tools.config import DIST_DIR
from gmeow_tools.graph import bind_prefixes
from gmeow_tools.transform import TransformReport, transform_graph
from gmeow_tools.up_projection import UpProjection, up_project
from gmeow_tools.up_projection_audit import _canon_qname
from gmeow_tools.up_projection_descend import up_project_descend

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path


@dataclass(slots=True)
class TranspileReport:
    """The result of a full transpile.

    The up-projection account + the maximal transform report, plus the path of
    the pure-GMEOW draft written alongside.
    """

    lifted: int  # source triples lifted as bare facts
    claimed: int  # source triples lifted as provenance-stamped claims
    context_resolved: int  # edges the descent resolved by graph position
    tag_resolved: int  # gmeow:hasTag edges added by the QID-bridge pass
    gap_terms: int  # distinct source terms with no lift rule
    ambiguous_terms: int  # distinct source terms held out as ambiguous
    draft_path: Path  # the pure-GMEOW intermediate
    gap_report_path: Path  # the gap report (un-lifted source triples)
    transform: TransformReport  # the MAXIMAL(G) report


def _gap_report(source: Graph, lift: UpProjection, stem: str) -> str:
    """Render a Markdown gap report — every un-lifted source triple.

    Never silently dropped: a triple is un-lifted because its term has **no lift
    rule** (a coverage gap) or its reverse is **ambiguous** (many gmeow
    up-targets, held out rather than guessed). Each is listed under its term.
    """
    gaps, ambig = lift.gap_terms, lift.ambiguous_terms
    held: dict[str, list[tuple[str, str, str]]] = {}
    for s, p, o in source:
        if not isinstance(p, URIRef):
            continue
        is_type = p == RDF.type and isinstance(o, URIRef)
        term = _canon_qname(str(o)) if is_type else _canon_qname(str(p))
        if term in gaps or term in ambig:
            held.setdefault(term, []).append((s.n3(), p.n3(), o.n3()))

    lines = [f"# Transpile gap report — {stem}\n"]
    lines.append(
        f"Lifted **{lift.lifted}** facts + **{lift.claimed}** claims. "
        f"The terms below could not be faithfully lifted to GMEOW — recorded "
        f"here, never silently dropped.\n"
    )

    def section(title: str, terms: dict[str, int], why: str) -> None:
        total = sum(terms.values())
        lines.append(f"## {title} — {total} triples / {len(terms)} terms\n")
        lines.append(f"_{why}_\n")
        if not terms:
            lines.append("(none)\n")
            return
        lines.append("| term | triples |")
        lines.append("|---|---|")
        for term, n in sorted(terms.items(), key=lambda kv: (-kv[1], kv[0])):
            lines.append(f"| `{term}` | {n} |")
        lines.append("")

    section("Gap terms", gaps, "no GMEOW lift rule — a coverage gap")
    section(
        "Ambiguous terms",
        ambig,
        "several gmeow up-targets, held out rather than guessed",
    )

    if held:
        lines.append("## Un-lifted source triples\n")
        for term in sorted(held):
            lines.append(f"### `{term}`\n")
            lines.append("```turtle")
            lines.extend(f"{s} {p} {o} ." for s, p, o in sorted(held[term]))
            lines.append("```\n")
    return "\n".join(lines)


def transpile(
    source_path: Path,
    *,
    out_dir: Path | None = None,
    profiles: Sequence[str] | None = None,
    descend: bool = True,
) -> TranspileReport:
    """Transpile a consumer-vocabulary source *file* to MAXIMAL GMEOW.

    Args:
        source_path: A non-GMEOW source RDF file (Turtle) to ingest.
        out_dir: Output directory (default ``dist/transpile/<stem>/``); receives
            the ``<stem>.gmeow.ttl`` draft and the maximal file family.
        profiles: Projection profiles for the maximal pass (default: all).
        descend: Use the context-aware graph-descent up-projection (default) over
            the per-term floor.

    Returns:
        The :class:`TranspileReport`.
    """
    source = Graph()
    source.parse(source_path, format="turtle")
    return transpile_graph(
        source, source_path.stem, out_dir=out_dir, profiles=profiles, descend=descend
    )


def transpile_graph(
    source: Graph,
    stem: str,
    *,
    out_dir: Path | None = None,
    profiles: Sequence[str] | None = None,
    descend: bool = True,
) -> TranspileReport:
    """Transpile an in-memory consumer-vocabulary graph to MAXIMAL GMEOW.

    The graph-core of :func:`transpile` — used by the CLI so a stdin-piped source
    (``gmeow transpile -``) flows through without a temp file.

    Args:
        source: The non-GMEOW source RDF graph to ingest.
        stem: The output basename (the draft, ``.gts`` file, default sub-dir).
        out_dir: Output directory (default ``dist/transpile/<stem>/``).
        profiles: Projection profiles for the maximal pass (default: all).
        descend: Use the context-aware graph-descent up-projection (default).

    Returns:
        The :class:`TranspileReport`.

    Raises:
        ValueError: If ``stem`` is empty/blank, or nothing lifts to GMEOW (an
            empty pure-GMEOW draft has nothing to project — surfaced, not a
            silent empty publication).
    """
    if not stem.strip():
        raise ValueError("transpile_graph: stem must be a non-empty string")
    target = out_dir if out_dir is not None else DIST_DIR / "transpile" / stem

    lift = up_project_descend(source) if descend else up_project(source)
    if len(lift.graph) == 0:
        msg = f"transpile: nothing lifted to GMEOW from {stem} — empty draft"
        raise ValueError(msg)

    target.mkdir(parents=True, exist_ok=True)
    # serialize lift.graph directly (no O(N) copy); transform_graph skolemizes
    # into a fresh graph, so it never mutates this one.
    bind_prefixes(lift.graph)
    draft_path = target / f"{stem}.gmeow.ttl"
    lift.graph.serialize(destination=draft_path, format="turtle")

    gap_report_path = target / f"{stem}.gaps.md"
    gap_report_path.write_text(_gap_report(source, lift, stem), encoding="utf-8")

    report = transform_graph(lift.graph, stem, out_dir=target, profiles=profiles)

    return TranspileReport(
        lifted=lift.lifted,
        claimed=lift.claimed,
        context_resolved=lift.context_resolved,
        tag_resolved=lift.tag_resolved,
        gap_terms=len(lift.gap_terms),
        ambiguous_terms=len(lift.ambiguous_terms),
        draft_path=draft_path,
        gap_report_path=gap_report_path,
        transform=report,
    )
