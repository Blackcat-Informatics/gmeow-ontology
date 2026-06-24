# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Lane-purity seal (#667, Principle 18): required lanes carry no Java/Docker.

Principle 18 splits the toolchain into two lanes that may not bleed into each
other. The PRIMARY lane — ``make check``, the required CI ``quality`` aggregator,
the build, and runtime — is rust-first and carries **no Java and no Docker**: it
reasons with the native ``gmeow_logic`` EL/DL engine, serializes RDF-1.2 with the
native ``gmeow-rdf`` codec (#667), and validates with native ``gmeow_shacl``. The
``classic-cross-check`` lane — ``make maint-classic-cross-check`` and a single,
deliberately **non-required** CI job — is the *sole* Java+Docker surface, where
the legacy oracles (ELK, HermiT, ROBOT, Jena) live.

Until now that separation was enforced only by comments in the Makefile and
``ci.yml`` plus pytest ``-m`` exclusions — a prose seal that rots silently. This
module turns the seal into an executable guard: it parses the workflows and the
Makefile and FAILS if a Docker/Java token (a ``docker`` command, the pinned
ROBOT/Jena images, ``--mode docker``, or a lane-only helper script) appears
anywhere the required path can reach, or if the ``classic-cross-check`` workflow
ever becomes a required check.

Pure-Python and offline — runs in the required CI ``python`` job, and is wired as
``meta:tests-lane-purity`` enforcing Principle 18 in ``governance/constitution.ttl``.
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

import yaml

from gmeow_tools.config import PROJECT_ROOT

WORKFLOWS = PROJECT_ROOT / ".github" / "workflows"
CI = WORKFLOWS / "ci.yml"
CLASSIC_CROSS_CHECK = WORKFLOWS / "classic-cross-check.yml"
MAKEFILE = PROJECT_ROOT / "Makefile"

#: Make targets allowed to invoke Docker/Java — the classic-cross-check lane and
#: its single-purpose siblings. Every other target must be Docker/Java-free.
LANE_MAKE_TARGETS: frozenset[str] = frozenset(
    {
        "maint-classic-cross-check",
        "maint-reason-hermit",
        "maint-explain",
        "maint-verify-docker",
        "maint-reasoning-cases",
        "maint-statements-docker-check",
        "maint-pull-images",
    }
)

#: Lane targets whose recipes MUST carry a Docker/Java token. This honesty pin
#: keeps the token detector from silently going vacuous (e.g. a regex that stops
#: matching after a tooling change would otherwise let everything "pass").
LANE_TARGETS_THAT_MUST_HIT: frozenset[str] = frozenset(
    {
        "maint-classic-cross-check",
        "maint-reason-hermit",
        "maint-verify-docker",
        "maint-reasoning-cases",
        "maint-statements-docker-check",
        "maint-pull-images",
    }
)

#: Lane-only helper scripts; a reference to one is itself a Docker/Java tell.
_LANE_SCRIPTS: tuple[str, ...] = (
    "reasoning_cases.py",
    "statements_docker_check.py",
    "slme_cross_check.py",
    "pull-images.sh",
)

#: A `docker` command (run/pull/build/image/compose), the `--mode docker` switch,
#: the two pinned oracle images, and actual JVM execution. Deliberately NOT a
#: bare `java`: the `rust` CI job legitimately uses `reporter: java-junit` and
#: `junit.xml`, which name a report format, not the JVM. We therefore match only
#: a launched JVM (`java -jar`/`java -cp`), the compiler (`javac`), or the build
#: tool (`gradle`/`gradlew`) — an actual invocation, never the substring `java`.
_DOCKER_PATTERNS: tuple[str, ...] = (
    r"\bdocker\s+(?:run|pull|build|image|compose)\b",
    r"--mode\s+docker",
    r"obolibrary/robot",
    r"stain/jena",
    r"\bjava\s+-(?:jar|cp)\b",
    r"\b(?:javac|gradlew?)\b",
)


def _forbidden_hits(text: str) -> set[str]:
    """Every Docker/Java token found in ``text`` (empty set means clean).

    Matching is case-insensitive: ``--reasoner elk`` and ``--reasoner ELK`` are
    the same invocation, so a lowercase variant must not slip through.
    """
    hits = {pat for pat in _DOCKER_PATTERNS if re.search(pat, text, re.IGNORECASE)}
    lowered = text.lower()
    hits |= {script for script in _LANE_SCRIPTS if script.lower() in lowered}
    return hits


def _load_yaml(path: Path) -> Any:
    """Parse a workflow file. YAML is inherently dynamic, hence ``Any``."""
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def _triggers(workflow: Any) -> dict[str, Any]:
    """The ``on:`` block. PyYAML parses the bare key ``on`` as ``True`` (YAML 1.1)."""
    on = workflow.get(True, workflow.get("on"))
    assert isinstance(on, dict), f"unexpected `on:` shape: {on!r}"
    return on


def _recipes() -> dict[str, list[str]]:
    """Map each Makefile target to its recipe lines (the TAB-indented body).

    Variable assignments (``X := …``), ``.PHONY`` lines, and comments are not
    targets and own no recipe; their text is never attributed to a recipe, so
    the lane comments describing Docker/Java are correctly ignored.
    """
    target = re.compile(r"^([A-Za-z][A-Za-z0-9_-]*)\s*:(?!=)")
    recipes: dict[str, list[str]] = {}
    current: str | None = None
    for line in MAKEFILE.read_text(encoding="utf-8").splitlines():
        if line.startswith("\t"):
            if current is not None:
                recipes[current].append(line)
            continue
        match = target.match(line)
        if match:
            name = match.group(1)
            recipes.setdefault(name, [])
            current = name
        elif line.strip() and not line.startswith("#"):
            # A non-tab, non-target, non-comment line (e.g. a variable) ends the
            # preceding recipe; nothing after it belongs to that target.
            current = None
    return recipes


def test_required_ci_jobs_carry_no_java_or_docker() -> None:
    """The required ``quality`` aggregator's jobs invoke no Docker/Java.

    The job set is read from ``jobs.quality.needs`` so a rename of the parallel
    jobs cannot quietly drop one from the guard.
    """
    ci = _load_yaml(CI)
    jobs = ci["jobs"]
    assert isinstance(jobs, dict)
    needs = jobs["quality"]["needs"]
    assert isinstance(needs, list) and needs, "quality.needs must list the gate jobs"

    for job_name in [*needs, "quality"]:
        job = jobs[job_name]
        assert isinstance(job, dict), f"{job_name} job is malformed"
        blobs: list[str] = []
        for step in job.get("steps", []):
            for key in ("name", "run", "uses"):
                value = step.get(key)
                if isinstance(value, str):
                    blobs.append(value)
            with_block = step.get("with")
            if isinstance(with_block, dict):
                blobs.extend(str(v) for v in with_block.values())
        blob = "\n".join(blobs)
        hits = _forbidden_hits(blob)
        assert not hits, f"required CI job {job_name!r} reaches Docker/Java: {hits}"
        # Dispatching the oracle lane by name (or naming an oracle reasoner) is
        # also forbidden on the required path. Match an actual invocation —
        # `make maint-classic-cross-check`, an oracle `--reasoner` switch — not a bare
        # mention, so the aggregator's "…oracle lane is non-blocking…" log line
        # and the required `crosscheck-queries` CLI command do not false-trip.
        # Lowercased so `--reasoner ELK` and `--reasoner elk` both trip.
        lowered_blob = blob.lower()
        oracle_tokens = (
            "make maint-classic-cross-check",
            "--reasoner hermit",
            "--reasoner elk",
        )
        for token in oracle_tokens:
            assert token not in lowered_blob, (
                f"required CI job {job_name!r} invokes the oracle lane: {token!r}"
            )


def test_classic_cross_check_workflow_is_never_required() -> None:
    """The Java/Docker lane runs only on a schedule, dispatch, or PR label.

    A ``push`` trigger would make it run unconditionally on the protected branch
    and edge it toward "required"; an unlabelled ``pull_request`` would run it on
    every PR. Both are forbidden — the lane must stay opt-in.
    """
    ccc = _load_yaml(CLASSIC_CROSS_CHECK)
    triggers = _triggers(ccc)
    assert "push" not in triggers, "the oracle lane must not run on push"
    assert set(triggers) <= {"schedule", "workflow_dispatch", "pull_request"}, (
        f"unexpected trigger(s) on the oracle lane: {sorted(triggers)}"
    )
    if "pull_request" in triggers:
        jobs = ccc["jobs"]
        assert isinstance(jobs, dict)
        assert all("label" in str(job.get("if", "")) for job in jobs.values()), (
            "EVERY pull_request-triggered oracle-lane job must gate on a label "
            "(a single ungated job would run Docker/Java on every PR)"
        )

    # And it is structurally absent from the required aggregator's needs: a
    # cross-workflow job can never appear in `quality.needs`, but assert it so a
    # future refactor that inlines the lane into ci.yml trips this guard.
    ci = _load_yaml(CI)
    needs = ci["jobs"]["quality"]["needs"]
    assert "classic-cross-check" not in needs


def test_makefile_required_targets_carry_no_java_or_docker() -> None:
    """Every Makefile target outside the lane allow-list is Docker/Java-free."""
    for target, lines in _recipes().items():
        if target in LANE_MAKE_TARGETS:
            continue
        hits = _forbidden_hits("\n".join(lines))
        assert not hits, (
            f"non-lane Makefile target {target!r} reaches Docker/Java: {hits}"
        )


def test_make_check_invokes_no_lane_target() -> None:
    """The required ``make check`` gate dispatches only Docker/Java-free targets."""
    recipes = _recipes()
    assert "check" in recipes, "the `check` target vanished"
    invoked: set[str] = set()
    for line in recipes["check"]:
        if "$(MAKE)" in line:
            invoked.update(re.findall(r"[A-Za-z][A-Za-z0-9_-]*", line))
    intruders = invoked & LANE_MAKE_TARGETS
    assert not intruders, f"`make check` invokes oracle-lane target(s): {intruders}"


def test_lane_surfaces_actually_carry_the_tokens() -> None:
    """Honesty pin: the detector fires on the real lane surfaces.

    Without this, a token set that silently stopped matching would make every
    "is clean" assertion above pass vacuously.
    """
    recipes = _recipes()
    for target in LANE_TARGETS_THAT_MUST_HIT:
        assert target in recipes, f"expected lane target {target!r} is gone"
        hits = _forbidden_hits("\n".join(recipes[target]))
        assert hits, f"lane target {target!r} no longer carries a Docker/Java token"

    workflow_text = CLASSIC_CROSS_CHECK.read_text(encoding="utf-8")
    assert (
        _forbidden_hits(workflow_text)
        or "make maint-classic-cross-check" in workflow_text
    ), "the classic-cross-check workflow no longer invokes the Docker/Java lane"
