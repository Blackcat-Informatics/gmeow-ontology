# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""OKF (Open Knowledge Format) import — the lift lane of the agent surface (#780).

The mirror of :mod:`gmeow_tools.okf_export`: an OKF Markdown bundle (the form an
LLM or human authors) is lifted back into GMEOW. The fold from Markdown to RDF is
the Rust ``gts from-okf`` primitive — we **never re-implement that codec** here
(the seam doctrine: ``gts`` owns the OKF↔graph conversion; gmeow owns the
ontology lift). This module shells the binary, then lifts the recognized ``okf:``
predicates into the standard ``rdfs:`` / ``skos:`` / ``rdf:`` surface.

OKF is a LOSSY surface, so the lift is honest about its bounds: the recognized
subset (``okf:title`` → ``rdfs:label``, ``okf:description`` → ``skos:definition``,
``okf:type`` → ``rdf:type``, ``okf:scope_notes`` / ``okf:examples`` → the SKOS
documentation predicates) is lifted; everything else (``okf:body``, ``okf:path``,
``okf:tag``, ``okf:links`` reifications, and the structured ``okf:<key>``
extensions) is **retained verbatim** as ``okf:`` annotations — self-identifying
provenance, never silently dropped (the ``_unmapped.nq`` honesty rule, mirrored
on the lift side).
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from rdflib import RDF, RDFS, SKOS, Graph, Literal, URIRef

from gmeow_tools.config import PROJECT_ROOT

if TYPE_CHECKING:
    from collections.abc import Sequence

    from gmeow_tools.language_tags import LangSelector
    from gmeow_tools.transform import TransformReport

#: The ``okf:`` profile namespace the Rust ``gts`` primitive folds to.
OKF_NS = "https://blackcatinformatics.ca/projects/gts/okf#"
_OWL = "http://www.w3.org/2002/07/owl#"

#: ``okf:type`` string literal → the rdf:type IRI it lifts to.
_TYPE_TO_RDF = {
    "Class": URIRef(_OWL + "Class"),
    "Property": URIRef(RDF.Property),
    "Individual": URIRef(_OWL + "NamedIndividual"),
}

#: ``okf:<key>`` → a single-valued standard predicate (literal carried straight).
_SCALAR_LIFT = {
    OKF_NS + "title": RDFS.label,
    OKF_NS + "description": SKOS.definition,
}

#: ``okf:<key>`` (an ``okf:json`` string list) → a multi-valued SKOS predicate.
_JSON_LIST_LIFT = {
    OKF_NS + "scope_notes": SKOS.scopeNote,
    OKF_NS + "examples": SKOS.example,
}


@dataclass(slots=True)
class OkfLiftReport:
    """Account of an OKF → GMEOW lift."""

    subjects: int  # distinct OKF document subjects seen
    lifted: int  # triples lifted to the rdfs/skos/rdf surface
    retained: int  # okf: triples kept verbatim as lossy annotations


@dataclass(slots=True)
class OkfTranspileReport:
    """The result of transpiling an OKF bundle directory to GMEOW."""

    lift: OkfLiftReport
    draft_path: Path  # the pure-GMEOW intermediate
    transform: TransformReport  # the MAXIMAL(G) report


class OkfBinaryNotFoundError(RuntimeError):
    """The ``gts`` binary with OKF support could not be located."""


def find_gts_binary() -> Path:
    """Locate the ``gts`` CLI (built ``--features okf``). HARD FAIL if absent.

    Resolution order: ``$GMEOW_GTS_BIN`` → ``gts`` on ``PATH`` → the sibling
    ``gmeow-gts`` Rust target dirs. No degraded fallback — OKF import requires the
    Rust codec (rust-first; the consumed Python ``gts`` package carries no OKF
    surface), so a missing binary is a hard error with a clear remedy.
    """
    env = os.environ.get("GMEOW_GTS_BIN")
    if env:
        candidate = Path(env)
        if candidate.is_file():
            return candidate
        msg = f"GMEOW_GTS_BIN={env} is not a file"
        raise OkfBinaryNotFoundError(msg)
    on_path = shutil.which("gts")
    if on_path:
        return Path(on_path)
    for rel in ("target/release/gts", "target/debug/gts"):
        candidate = PROJECT_ROOT.parent / "gmeow-gts" / "rust" / rel
        if candidate.is_file():
            return candidate
    msg = (
        "gts binary with OKF support not found. Build it with "
        "`cargo build --release --features okf --bin gts` in the gmeow-gts repo "
        "and point GMEOW_GTS_BIN at the resulting binary (or put it on PATH)."
    )
    raise OkfBinaryNotFoundError(msg)


def okf_dir_to_graph(okf_dir: Path, *, gts_bin: Path | None = None) -> Graph:
    """Fold an OKF bundle directory to an rdflib graph via ``gts from-okf``.

    Shells the Rust primitive (the only OKF→graph codec), writing a temporary GTS
    snapshot, then reads its default graph back. The ``okf:`` reification of body
    links rides as RDF-1.2 quoted terms, which the compatibility reader drops —
    the asserted ``okf:`` metadata triples (the lift's input) come through intact.
    """
    binary = gts_bin if gts_bin is not None else find_gts_binary()
    from gmeow_tools.describe import load_graph_from_gts

    prefix = ".gmeow-tmp-okfin-"
    with tempfile.TemporaryDirectory(dir=PROJECT_ROOT, prefix=prefix) as tmp:
        out = Path(tmp) / "from-okf.gts"
        proc = subprocess.run(
            [str(binary), "from-okf", str(okf_dir), "-o", str(out)],
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            msg = f"gts from-okf failed ({proc.returncode}): {proc.stderr.strip()}"
            raise RuntimeError(msg)
        return load_graph_from_gts(out)


def lift_okf_graph(source: Graph) -> tuple[Graph, OkfLiftReport]:
    """Lift recognized ``okf:`` predicates to GMEOW; retain the rest as annotations.

    Returns the lifted graph plus an :class:`OkfLiftReport`. The recognized subset
    becomes ``rdfs:label`` / ``skos:definition`` / ``rdf:type`` /
    ``skos:scopeNote`` / ``skos:example``; every other ``okf:`` triple is kept
    verbatim (lossy honesty), and non-``okf:`` triples pass through unchanged.
    """
    out = Graph()
    subjects: set[object] = set()
    lifted = 0
    retained = 0
    okf_type = URIRef(OKF_NS + "type")
    okf_resource = URIRef(OKF_NS + "resource")
    for s, p, o in source:
        if isinstance(p, URIRef) and str(p).startswith(OKF_NS):
            subjects.add(s)
            key = str(p)
            if p == okf_type and isinstance(o, Literal):
                rdf_type = _TYPE_TO_RDF.get(str(o))
                if rdf_type is not None:
                    out.add((s, RDF.type, rdf_type))
                    lifted += 1
                    continue
            elif key in _SCALAR_LIFT:
                out.add((s, _SCALAR_LIFT[key], o))
                lifted += 1
                continue
            elif key in _JSON_LIST_LIFT and isinstance(o, Literal):
                for item in _json_list(o):
                    out.add((s, _JSON_LIST_LIFT[key], Literal(item)))
                    lifted += 1
                continue
            elif p == okf_resource:
                # The subject already IS the resource IRI (gts from-okf mints it
                # from resource:); the explicit okf:resource triple is redundant
                # identity — drop it rather than retain a self-reference.
                continue
            # Unmapped okf:* — retained verbatim as a provenance-bearing annotation.
            out.add((s, p, o))
            retained += 1
        else:
            out.add((s, p, o))
    return out, OkfLiftReport(subjects=len(subjects), lifted=lifted, retained=retained)


def transpile_okf(
    okf_dir: Path,
    *,
    out_dir: Path | None = None,
    profiles: Sequence[str] | None = None,
    selector: LangSelector | None = None,
    gts_bin: Path | None = None,
) -> OkfTranspileReport:
    """Transpile an OKF bundle directory to MAXIMAL GMEOW.

    ``gts from-okf`` folds the Markdown bundle, the recognized ``okf:`` predicates
    are lifted to GMEOW (unmapped ones retained as annotations), the pure-GMEOW
    draft is written, then ``MAXIMAL(G)`` is run over it — the same back half as
    the Turtle / YAML-LD transpile paths, so an OKF source is re-expressed across
    every vocabulary GMEOW can reach.

    Raises:
        ValueError: If nothing lifts to GMEOW (an empty draft has nothing to
            project — surfaced, not a silent empty publication).
    """
    from gmeow_tools.config import DIST_DIR
    from gmeow_tools.graph import bind_prefixes
    from gmeow_tools.transform import transform_graph

    graph = okf_dir_to_graph(okf_dir, gts_bin=gts_bin)
    lifted, report = lift_okf_graph(graph)
    if report.lifted == 0:
        msg = f"transpile: nothing lifted to GMEOW from OKF bundle {okf_dir}"
        raise ValueError(msg)

    stem = okf_dir.name or "okf"
    target = out_dir if out_dir is not None else DIST_DIR / "transpile" / stem
    target.mkdir(parents=True, exist_ok=True)
    bind_prefixes(lifted)
    draft_path = target / f"{stem}.gmeow.ttl"
    lifted.serialize(destination=draft_path, format="turtle")

    transform = transform_graph(
        lifted, stem, out_dir=target, profiles=profiles, selector=selector
    )
    return OkfTranspileReport(lift=report, draft_path=draft_path, transform=transform)


def _json_list(literal: Literal) -> list[str]:
    """Parse an ``okf:json`` list literal into its string items (best-effort)."""
    try:
        value = json.loads(str(literal))
    except (ValueError, TypeError):
        return [str(literal)]
    if isinstance(value, list):
        return [str(item) for item in value]
    return [str(value)]
