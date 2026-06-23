# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Machine-readable compliance report (#285): per-principle gate evidence.

The constitution manifest (#280) knows which gates enforce which principles;
this module RUNS the in-process gates and serializes the per-principle
results as RDF — FAIR/FOOPS scoring and the publication metadata get
evidence rather than assertion, and every release carries a proof object of
what was enforced, at what version, with what result.

Statuses per enforcement:

* ``passed`` / ``failed`` — at least one of its make targets / CLI commands
  maps to an in-process runner, and the conjunction of those runners' error
  counts decides;
* ``gated-in-ci`` — the enforcement is real but runs outside this process
  (pytest suites, the Docker reasoners); its EXISTENCE is still verified by
  ``constitution-check``;
* ``declared`` — practice or artifact-only enforcement; existence-verified.

The report is a runtime artifact (it embeds run results), so it lives in
``dist/`` — never under the drift-gated ``generated/`` tree.
"""

from __future__ import annotations

import datetime
import platform
import subprocess
from dataclasses import dataclass
from typing import TYPE_CHECKING

from gmeow_tools import __version__
from gmeow_tools.config import DIST_DIR, PROJECT_ROOT
from gmeow_tools.constitution import Manifest, load_manifest
from gmeow_tools.validate import ValidationResult

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping
    from pathlib import Path

META = "https://blackcatinformatics.ca/gmeow/meta#"
REPORT_FILE = DIST_DIR / "compliance-report.ttl"


@dataclass(frozen=True, slots=True)
class GateRun:
    """The outcome of one executed gate."""

    errors: int
    warnings: int | None


def _run_validate() -> ValidationResult:
    from gmeow_tools.validate import validate_all

    return validate_all()


def _run_constitution() -> ValidationResult:
    from gmeow_tools.constitution import check_constitution

    return check_constitution()


def _run_alignment() -> ValidationResult:
    from gmeow_tools.alignment_lint import (
        findings_to_result,
        lint_alignment_directions,
    )

    return findings_to_result(lint_alignment_directions(allow_network=False))


def _run_check_generated() -> ValidationResult:
    import os

    result = ValidationResult()
    try:
        import gmeow_native.pipeline as _pipeline  # type: ignore[import-not-found]
    except ImportError as exc:
        result.warnings.append(f"generated drift not checked here: {exc}")
        return result

    # The Rust pipeline (the build authority since #861 P7) reproduces every
    # committed artifact single-pass and reports any drift in CHECK mode.
    report = _pipeline.run_pipeline(str(PROJECT_ROOT), os.cpu_count() or 1, True)
    for rel in sorted(report.get("drifted", [])):
        result.errors.append(f"drift: {rel}")
    for finding in report.get("findings", []):
        if finding["severity"] == "error":
            result.errors.append(f"{finding['code']}: {finding['message']}")
    return result


#: In-process runners, keyed by the make target / CLI command an enforcement
#: cites. The manifest's own citations select the runner — no second mapping.
RUNNERS: dict[str, Callable[[], ValidationResult]] = {
    "validate": _run_validate,
    "constitution-check": _run_constitution,
    "lint-alignment": _run_alignment,
    "check-generated": _run_check_generated,
}


def run_gates(names: frozenset[str] | None = None) -> dict[str, GateRun]:
    """Execute the in-process runners (each at most once)."""
    results: dict[str, GateRun] = {}
    for name, runner in RUNNERS.items():
        if names is not None and name not in names:
            continue
        outcome = runner()
        results[name] = GateRun(len(outcome.errors), len(outcome.warnings))
    return results


def assumed_passed_gate_runs(names: frozenset[str] | None = None) -> dict[str, GateRun]:
    """Return pass evidence for gates already run by the surrounding workflow."""
    return {
        name: GateRun(errors=0, warnings=None)
        for name in RUNNERS
        if names is None or name in names
    }


def _git_head() -> str:
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=PROJECT_ROOT,
            capture_output=True,
            text=True,
            check=True,
            timeout=10,
        ).stdout.strip()
    except Exception:
        return "unknown"


def _enforcement_status(
    citations: tuple[str, ...], kind: str, gate_runs: Mapping[str, GateRun]
) -> tuple[str, int, int | None]:
    """(status, errors, warnings) for one enforcement's citations."""
    # dict.fromkeys-dedupe: an enforcement may cite the same runnable as both
    # makeTarget and cliCommand; its run must be counted once.
    ran = [gate_runs[c] for c in dict.fromkeys(citations) if c in gate_runs]
    if ran:
        errors = sum(r.errors for r in ran)
        warning_counts: list[int] = []
        warnings_unknown = False
        for run in ran:
            if run.warnings is None:
                warnings_unknown = True
            else:
                warning_counts.append(run.warnings)
        warnings = None if warnings_unknown else sum(warning_counts)
        return ("failed" if errors else "passed", errors, warnings)
    if kind in ("TestSuite", "Gate", "Shape", "Lint"):
        return ("gated-in-ci", 0, 0)
    return ("declared", 0, 0)


def build_report(
    manifest: Manifest,
    gate_runs: Mapping[str, GateRun],
    *,
    generated_at: str,
    source_commit: str,
    evidence_mode: str = "in-process",
) -> str:
    """Render the compliance report as Turtle (pure; testable with fakes)."""
    lines = [
        "# GMEOW compliance report (#285) — per-principle gate evidence.",
        "# A runtime proof object, regenerated by `gmeow compliance-report`.",
        "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .",
        "@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .",
        "",
        "meta:report a meta:ComplianceReport ;",
        f'    meta:generatedAt "{generated_at}"^^xsd:dateTime ;',
        f'    meta:sourceCommit "{source_commit}" ;',
        f'    meta:toolchainVersion "{__version__}" ;',
        f'    meta:pythonVersion "{platform.python_version()}" ;',
        f'    meta:evidenceMode "{evidence_mode}" ;',
        "    meta:assesses "
        + ", ".join(f"meta:Principle{p.number}" for p in manifest.principles)
        + " .",
        "",
    ]
    for principle in manifest.principles:
        statuses: list[str] = []
        body: list[str] = []
        for iri in principle.enforced_by:
            enforcement = manifest.enforcements.get(iri)
            if enforcement is None:
                continue
            name = iri.removeprefix(META)
            citations = (*enforcement.make_targets, *enforcement.cli_commands)
            status, errors, warnings = _enforcement_status(
                citations, enforcement.kind, gate_runs
            )
            statuses.append(status)
            warning_count = (
                "" if warnings is None else f" ; meta:warningCount {warnings}"
            )
            body.append(
                f"    meta:enforcementResult [ meta:enforcement meta:{name} ; "
                f'meta:status "{status}" ; meta:errorCount {errors}'
                f"{warning_count} ]"
            )
        relations: list[str] = []
        if principle.superseded_in_part_by:
            relations.append(
                "    meta:supersededInPartBy "
                + ", ".join(
                    f"meta:Principle{n}" for n in principle.superseded_in_part_by
                )
            )
        if principle.extends:
            relations.append(
                "    meta:extends "
                + ", ".join(f"meta:Principle{n}" for n in principle.extends)
            )
        # Carry the supersession edges through to the report (north-star: maximal
        # information flow); prepend so the enforcementResults stay block-terminal.
        body = relations + body
        overall = "failed" if "failed" in statuses else "passed"
        lines.append(f"meta:Principle{principle.number}Result")
        lines.append("    a meta:PrincipleResult ;")
        lines.append(f"    meta:principle meta:Principle{principle.number} ;")
        lines.append(f'    meta:status "{overall}" ;')
        lines.extend(f"{b} ;" for b in body[:-1])
        if body:
            lines.append(f"{body[-1]} .")
        else:
            lines[-1] = lines[-1][:-1] + "."
        lines.append("")
    return "\n".join(lines)


def compliance_report(*, assume_runners_passed: bool = False) -> str:
    """Run the gates and render the full report."""
    manifest = load_manifest()
    gate_runs = assumed_passed_gate_runs() if assume_runners_passed else run_gates()
    return build_report(
        manifest,
        gate_runs,
        generated_at=datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
        source_commit=_git_head(),
        evidence_mode=(
            "prior-successful-gates" if assume_runners_passed else "in-process"
        ),
    )


def write_report(
    path: Path = REPORT_FILE, *, assume_runners_passed: bool = False
) -> Path:
    """Write the report to ``dist/`` and return the path."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        compliance_report(assume_runners_passed=assume_runners_passed),
        encoding="utf-8",
    )
    return path
