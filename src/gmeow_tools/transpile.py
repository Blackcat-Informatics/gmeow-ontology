# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
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

from rdflib import Graph

from gmeow_tools.config import DIST_DIR
from gmeow_tools.graph import bind_prefixes
from gmeow_tools.transform import TransformReport, transform_graph
from gmeow_tools.up_projection import up_project
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
    gap_terms: int  # distinct source terms with no lift rule
    ambiguous_terms: int  # distinct source terms held out as ambiguous
    draft_path: Path  # the pure-GMEOW intermediate
    transform: TransformReport  # the MAXIMAL(G) report


def transpile(
    source_path: Path,
    *,
    out_dir: Path | None = None,
    profiles: Sequence[str] | None = None,
    descend: bool = True,
) -> TranspileReport:
    """Transpile a consumer-vocabulary source file to MAXIMAL GMEOW.

    Args:
        source_path: A non-GMEOW source RDF file (Turtle) to ingest.
        out_dir: Output directory (default ``dist/transpile/<stem>/``); receives
            the ``<stem>.gmeow.ttl`` draft and the maximal file family.
        profiles: Projection profiles for the maximal pass (default: all).
        descend: Use the context-aware graph-descent up-projection (default) over
            the per-term floor.

    Returns:
        The :class:`TranspileReport`.

    Raises:
        ValueError: If the source graph is empty, or nothing lifts to GMEOW (an
            empty pure-GMEOW draft has nothing to project — surfaced, not a
            silent empty publication).
    """
    target = (
        out_dir if out_dir is not None else DIST_DIR / "transpile" / source_path.stem
    )

    source = Graph()
    source.parse(source_path, format="turtle")

    lift = up_project_descend(source) if descend else up_project(source)
    if len(lift.graph) == 0:
        msg = f"transpile: nothing lifted to GMEOW from {source_path} — empty draft"
        raise ValueError(msg)

    target.mkdir(parents=True, exist_ok=True)
    draft = Graph()
    bind_prefixes(draft)
    for triple in lift.graph:
        draft.add(triple)
    draft_path = target / f"{source_path.stem}.gmeow.ttl"
    draft.serialize(destination=draft_path, format="turtle")

    report = transform_graph(
        lift.graph, source_path.stem, out_dir=target, profiles=profiles
    )

    return TranspileReport(
        lifted=lift.lifted,
        claimed=lift.claimed,
        context_resolved=lift.context_resolved,
        gap_terms=len(lift.gap_terms),
        ambiguous_terms=len(lift.ambiguous_terms),
        draft_path=draft_path,
        transform=report,
    )
