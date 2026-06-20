"""Python facade for the Rust-owned GMEOW diagnostics core."""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any, cast

import gmeow_diagnostics
from rich.console import Console

from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.diagnostics_config import ConsoleMode, DiagnosticsConfig

DiagnosticsFinding = Any
DiagnosticsReport = Any


def finding(
    *,
    severity: str,
    code: str,
    message: str,
    tool: str | None = None,
    path: str | None = None,
    line: int | None = None,
    column: int | None = None,
    logical: str | None = None,
    detail: str | None = None,
    tags: Sequence[str] | None = None,
    suggestions: Sequence[str] | None = None,
) -> DiagnosticsFinding:
    """Build a native diagnostics finding."""
    return gmeow_diagnostics.Finding(
        severity,
        code,
        message,
        tool=tool,
        path=path,
        line=line,
        column=column,
        logical=logical,
        detail=detail,
        tags=list(tags or []),
        suggestions=list(suggestions or []),
    )


def report(tool: str) -> DiagnosticsReport:
    """Build an empty native diagnostics report."""
    return gmeow_diagnostics.Report(tool)


def report_from_json(data: str) -> DiagnosticsReport:
    """Rehydrate a diagnostics report from its canonical JSON form.

    The inverse of the Rust ``Report`` JSON serialization. Used by native lanes
    (e.g. ``gmeow_logic.verify_native``) that build the report in Rust and hand
    Python the JSON string to reconstruct (#695).
    """
    return gmeow_diagnostics.Report.from_json(data)


def report_from_messages(
    *,
    tool: str,
    errors: Sequence[str],
    warnings: Sequence[str],
) -> DiagnosticsReport:
    """Build a diagnostics report from legacy error and warning strings."""
    return gmeow_diagnostics.from_legacy(tool, list(errors), list(warnings))


def report_from_findings(
    *,
    tool: str,
    findings: Sequence[DiagnosticsFinding],
) -> DiagnosticsReport:
    """Build a diagnostics report from already-constructed native findings.

    The surface-agnostic primitive each dev-gate surface's
    ``to_diagnostics_report`` reuses: a surface maps its own result into a list
    of :func:`finding` objects (it owns the severity/code semantics) and folds
    them here. Keeps the facade free of any per-surface knowledge.
    """
    output = report(tool)
    for item in findings:
        output.add(item)
    return output


def report_from_validation_result(
    result: Any,
    *,
    tool: str = "validate",
) -> DiagnosticsReport:
    """Build a diagnostics report from a ``ValidationResult`` without changing it.

    When the result carries ``report_json`` — the single canonical report the
    Rust validation orchestration always emits (#654) — that report IS the
    source: it preserves SHACL focus nodes and GTS wire coordinates. A
    hand-built result without ``report_json`` (e.g. in tests, or a sub-lint)
    falls back to its legacy error/warning strings.
    """
    report_json = getattr(result, "report_json", None)
    if report_json:
        output = gmeow_diagnostics.Report.from_json(report_json)
    else:
        output = report_from_messages(
            tool=tool,
            errors=list(result.errors),
            warnings=list(result.warnings),
        )
    timings = list(getattr(result, "timings", []))
    if timings:
        output.set_metadata_json("timings", json.dumps(timings, sort_keys=True))
    return output


def emit_legacy_cli(report_obj: DiagnosticsReport, err_console: Console) -> None:
    """Print warnings and errors in the existing CLI style (the ``pretty`` mode)."""
    for warning in list(report_obj.warnings):
        err_console.print(f"[yellow]warning[/yellow] {warning}")
    for error in list(report_obj.errors):
        err_console.print(f"[red]error[/red] {error}")


def _findings_as_jsonl(report_obj: DiagnosticsReport) -> list[str]:
    """The report's findings as compact one-JSON-object-per-line strings.

    Sourced from the Rust-canonical ``report.to_json()`` (already normalized and
    ordered), not re-serialized field-by-field in Python — so JSONL is a faithful
    line-framing of the canonical projection and stays deterministic.
    """
    payload = json.loads(report_obj.to_json())
    return [
        json.dumps(item, sort_keys=True, separators=(",", ":"))
        for item in payload.get("findings", [])
    ]


def emit_console(
    report_obj: DiagnosticsReport,
    config: DiagnosticsConfig,
    err_console: Console,
) -> None:
    """Project a report to the console per the resolved console mode (#662).

    ``auto`` is already collapsed to a concrete mode during
    :meth:`DiagnosticsConfig.resolve`, so this only ever dispatches on
    pretty/text/jsonl/silent. ``silent`` prints nothing; an unhandled mode is a
    hard error (no silent fallback). Text/JSONL are printed with Rich markup and
    highlighting off so payload characters are emitted verbatim.
    """
    mode = config.console
    if mode is ConsoleMode.SILENT:
        return
    if mode is ConsoleMode.PRETTY:
        emit_legacy_cli(report_obj, err_console)
        return
    if mode is ConsoleMode.TEXT:
        text = report_obj.render_text()
        if text:
            err_console.print(text, markup=False, highlight=False)
        return
    if mode is ConsoleMode.JSONL:
        for line in _findings_as_jsonl(report_obj):
            err_console.print(line, markup=False, highlight=False)
        return
    raise ValueError(f"unhandled console mode: {mode}")


def write_report_artifacts(
    report_obj: DiagnosticsReport,
    *,
    output_dir: Path = PROJECT_ROOT / "dist",
    stem: str = "gmeow-feedback",
    artifacts: Sequence[str] | frozenset[str] | None = None,
) -> dict[str, Path]:
    """Write the selected diagnostics artifacts for a report.

    ``artifacts`` selects which projections to write (#662). ``None`` is the
    maximal default — all of JSON, SARIF, and HTML — preserving the behavior
    every existing caller relies on. An empty selection writes nothing (and
    deletes nothing), returning ``{}``. The Rust writer keeps its fixed
    json/sarif/html order regardless of the selection's order, so output is
    deterministic.
    """
    if artifacts is not None:
        kinds = sorted(artifacts)
        if not kinds:
            return {}
        raw_paths = cast(
            Mapping[str, str],
            report_obj.write_artifacts(str(output_dir), stem, kinds),
        )
    else:
        raw_paths = cast(
            Mapping[str, str],
            report_obj.write_artifacts(str(output_dir), stem),
        )
    return {kind: Path(path) for kind, path in raw_paths.items()}
