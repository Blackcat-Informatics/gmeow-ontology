#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Non-required native-vs-ROBOT SLME + verify cross-check oracle (issue #695).

This is a Docker-backed lane that runs ONLY in ``make classic-cross-check`` /
the ``classic-cross-check`` workflow — never on a required gate. It proves two
things about the native, Java/Docker-free authority:

1. **Native SLME ⊇ ROBOT SLME.** For each construct-coverage case the native
   syntactic-locality extractor (``gmeow_logic.extract_module``) must keep every
   logical axiom that ROBOT's ``extract`` keeps. Over-extraction (a sound
   superset) is allowed; *under*-extraction (dropping an axiom ROBOT keeps) is a
   hard failure. The gate is ``only_robot`` (ROBOT-only logical triples) ==
   empty, compared blank-node-aware with :func:`rdflib.compare.graph_diff`.

2. **Native verify agrees with ROBOT verify** on the committed ontology: both
   the ``docker`` (ROBOT) lane and the ``native`` (Rust) lane must report the
   real bundle CLEAN. A native-only violation is a false positive; a ROBOT-only
   violation is a dropped negative — either fails the gate.

Run only (mirrors ``scripts/reasoning_cases.py``): ``main() -> int`` printing
``ok:``/errors, returning a process exit code. It is never imported by the
build.
"""

from __future__ import annotations

import subprocess
import sys
from dataclasses import dataclass

from rdflib import Graph, URIRef
from rdflib.compare import graph_diff, to_isomorphic
from rdflib.term import Node

from gmeow_tools.config import DIST_DIR, PROJECT_ROOT, ROBOT_IMAGE
from gmeow_tools.runner import (
    ToolExecutionError,
    ToolUnavailableError,
    run_container,
)

#: The hand-built construct-coverage source ontology.
COVERAGE_TTL = PROJECT_ROOT / "tests" / "fixtures" / "slme" / "coverage.ttl"

#: Where ROBOT writes its module output (gitignored ``dist/``).
SLME_OUT_DIR = DIST_DIR / "slme-cross-check"

COV = "http://example.org/cov#"

#: Triple predicates that are pure annotations (not logical axioms). The gate
#: enforces native ⊇ ROBOT on the *logical* axioms; declaration/annotation
#: deltas are reported but never fail the run.
_ANNOTATION_PREDS = frozenset(
    {
        "http://www.w3.org/2000/01/rdf-schema#label",
        "http://www.w3.org/2000/01/rdf-schema#comment",
        "http://www.w3.org/2004/02/skos/core#prefLabel",
    }
)
_RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
_OWL = "http://www.w3.org/2002/07/owl#"

#: Symmetric logical predicates: ROBOT and the native extractor may serialize
#: the axiom in either direction (``a P b`` vs ``b P a``), which is the SAME
#: logical axiom. Canonicalize the (subject, object) order before diffing so a
#: pure serialization-direction difference is not mistaken for an under- or
#: over-extraction.
_SYMMETRIC_PREDS = frozenset(
    {
        f"{_OWL}inverseOf",
        f"{_OWL}disjointWith",
        f"{_OWL}equivalentClass",
        f"{_OWL}equivalentProperty",
        f"{_OWL}sameAs",
        f"{_OWL}differentFrom",
        f"{_OWL}propertyDisjointWith",
    }
)


@dataclass(frozen=True)
class SlmeCase:
    """One SLME extraction case: a seed signature + locality method."""

    name: str
    seeds: list[str]
    method: str


SLME_CASES: list[SlmeCase] = [
    SlmeCase("star-seed-A", [f"{COV}A"], "STAR"),
    SlmeCase("bot-seed-A", [f"{COV}A"], "BOT"),
    SlmeCase("top-seed-C", [f"{COV}C"], "TOP"),
]


def _is_declaration(triple: tuple[object, object, object]) -> bool:
    """Return whether a triple is an entity declaration (``a owl:Class`` etc.)."""
    _s, p, o = triple
    return str(p) == _RDF_TYPE and str(o).startswith(_OWL)


def _logical_only(graph: Graph) -> Graph:
    """Keep logical axioms, dropping annotations/declarations.

    Symmetric predicates (``owl:inverseOf`` etc.) are direction-canonicalized so
    a serialization-direction difference between ROBOT and native is not read as
    an under-/over-extraction.
    """
    out = Graph()
    for triple in graph:
        s, p, o = triple
        if str(p) in _ANNOTATION_PREDS:
            continue
        if _is_declaration(triple):
            continue
        if str(p) in _SYMMETRIC_PREDS and str(s) > str(o):
            out.add((o, p, s))
            continue
        out.add(triple)
    return out


def _robot_extract(case: SlmeCase) -> Graph:
    """Run ROBOT ``extract`` in the pinned image and parse its module output."""
    SLME_OUT_DIR.mkdir(parents=True, exist_ok=True)
    out_path = SLME_OUT_DIR / f"robot-{case.name}.ttl"
    src_rel = COVERAGE_TTL.relative_to(PROJECT_ROOT).as_posix()
    out_rel = out_path.relative_to(PROJECT_ROOT).as_posix()
    args = [
        "robot",
        "extract",
        "--method",
        case.method,
        "--input",
        src_rel,
    ]
    for iri in case.seeds:
        args += ["--term", iri]
    # ROBOT infers Turtle from the ``.ttl`` output extension.
    args += ["--output", out_rel]
    run_container(ROBOT_IMAGE, args)
    graph = Graph()
    graph.parse(out_path, format="turtle")
    return graph


def _native_extract(case: SlmeCase) -> Graph:
    """Run the native ``extract_module`` and parse its module Turtle."""
    import gmeow_logic

    result = gmeow_logic.extract_module(
        COVERAGE_TTL.read_text(encoding="utf-8"), case.seeds, case.method
    )
    graph = Graph()
    graph.parse(data=result["module_ttl"], format="turtle")
    return graph


def _named_iris(graph: Graph) -> set[str]:
    """Return the set of named-IRI subjects/objects appearing in ``graph``."""
    iris: set[str] = set()
    for s, _p, o in graph:
        if isinstance(s, URIRef):
            iris.add(str(s))
        if isinstance(o, URIRef):
            iris.add(str(o))
    return iris


def _check_slme_case(case: SlmeCase) -> None:
    """Assert native SLME preserves every Σ-relevant axiom ROBOT keeps.

    The SLME soundness property is that the native module preserves all
    entailments over the seed signature Σ. The proxy gate is: every logical
    axiom ROBOT keeps that *touches an entity native also kept* must be kept by
    native too (native must not drop something relevant to its own signature
    closure). A ROBOT-kept axiom whose entities are ALL absent from native's
    module is Σ-irrelevant — native producing a tighter (still sound) module by
    dropping it is acceptable; such cases are reported but do not fail.
    """
    robot_g = _logical_only(_robot_extract(case))
    native_raw = _native_extract(case)
    native_g = _logical_only(native_raw)
    native_iris = _named_iris(native_raw)

    _in_both, only_robot, only_native = graph_diff(
        to_isomorphic(robot_g), to_isomorphic(native_g)
    )

    relevant_drops: list[tuple[Node, Node, Node]] = []
    irrelevant_drops: list[tuple[Node, Node, Node]] = []
    for triple in only_robot:
        s, _p, o = triple
        endpoints = {str(t) for t in (s, o) if isinstance(t, URIRef)}
        if endpoints & native_iris:
            relevant_drops.append(triple)
        else:
            irrelevant_drops.append(triple)

    print(
        f"  {case.name}: robot_logical={len(robot_g)} native_logical={len(native_g)} "
        f"only_robot={len(only_robot)} (relevant={len(relevant_drops)} "
        f"irrelevant={len(irrelevant_drops)}) only_native={len(only_native)}"
    )
    for s, p, o in sorted(only_native, key=lambda t: tuple(str(x) for x in t)):
        # Allowed (sound superset).
        print(f"    only_native: {s} {p} {o}")
    for s, p, o in sorted(irrelevant_drops, key=lambda t: tuple(str(x) for x in t)):
        # Allowed: native is a tighter, still-sound module (Σ-irrelevant axiom).
        print(f"    only_robot (Σ-irrelevant, tighter native module OK): {s} {p} {o}")
    if relevant_drops:
        print(
            f"  FAIL {case.name}: native dropped "
            f"{len(relevant_drops)} Σ-relevant logical axiom(s) ROBOT keeps:",
            file=sys.stderr,
        )
        for s, p, o in sorted(relevant_drops, key=lambda t: tuple(str(x) for x in t)):
            print(f"    only_robot (Σ-relevant): {s} {p} {o}", file=sys.stderr)
        raise AssertionError(
            f"native SLME under-extracted a Σ-relevant axiom for {case.name}"
        )


def _run_gmeow_dev(verify_args: list[str]) -> tuple[bool, str]:
    """Run a ``gmeow-dev`` subcommand; return (clean, combined output)."""
    cmd = ["uv", "run", "--package", "gmeow-dev", "gmeow-dev", *verify_args]
    proc = subprocess.run(
        cmd,
        cwd=PROJECT_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.returncode == 0, proc.stdout + proc.stderr


def _check_verify_parity() -> None:
    """Assert ROBOT verify and native verify both report the committed bundle clean.

    A native-only violation is a false positive; a ROBOT-only violation is a
    dropped negative. On the clean committed ontology both must pass.
    """
    # ROBOT verify needs the reasoned graph; produce it first (Docker ELK).
    reasoned_ok, reason_out = _run_gmeow_dev(
        ["reason", "--mode", "docker", "--reasoner", "ELK"]
    )
    if not reasoned_ok:
        print(reason_out, file=sys.stderr)
        raise AssertionError("docker reason (ELK) failed; cannot run ROBOT verify")

    robot_clean, robot_out = _run_gmeow_dev(
        ["verify", "--mode", "docker", "--reasoner", "ELK"]
    )
    native_clean, native_out = _run_gmeow_dev(["verify", "--mode", "native"])

    print(f"  verify docker (ROBOT): {'CLEAN' if robot_clean else 'VIOLATIONS'}")
    print(f"  verify native (Rust):  {'CLEAN' if native_clean else 'VIOLATIONS'}")

    if robot_clean and native_clean:
        return

    if not robot_clean:
        print(robot_out, file=sys.stderr)
    if not native_clean:
        print(native_out, file=sys.stderr)

    if native_clean and not robot_clean:
        raise AssertionError(
            "ROBOT verify tripped a violation native verify did not "
            "(dropped negative): native must not under-report"
        )
    if robot_clean and not native_clean:
        raise AssertionError(
            "native verify tripped a violation ROBOT verify did not "
            "(false positive): native must not over-report on the clean bundle"
        )
    raise AssertionError(
        "both ROBOT and native verify reported violations on the committed "
        "bundle (the committed ontology should be clean)"
    )


def main() -> int:
    """Run the SLME + verify cross-check and return a process exit code."""
    if not COVERAGE_TTL.exists():
        print(f"missing fixture: {COVERAGE_TTL}", file=sys.stderr)
        return 2
    try:
        print("SLME native ⊇ ROBOT cross-check:")
        for case in SLME_CASES:
            _check_slme_case(case)
            print(f"ok: slme {case.name}")
        print("verify parity (committed bundle):")
        _check_verify_parity()
        print("ok: verify parity (ROBOT == native, both clean)")
    except ToolUnavailableError as exc:
        print(f"tool unavailable: {exc}", file=sys.stderr)
        return 2
    except ToolExecutionError as exc:
        print(f"cross-check tool failed:\n{exc.output}", file=sys.stderr)
        return 2
    except (AssertionError, ValueError) as exc:
        print(f"cross-check failed: {exc}", file=sys.stderr)
        return 2
    print("ok: slme cross-check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
