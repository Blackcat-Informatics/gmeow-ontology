# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Wrap external dev-gate tools as canonical diagnostics findings (#662).

The gate shells out to tools GMEOW does not own — ``pre-commit``, ``mypy``,
``pytest``, ``cargo``, ``clippy``, ``maturin``. When one of those fails, its raw
log should ride the same ``Finding``/``Report`` rail as every other surface so it
projects to the same SARIF/JSON/HTML and CI code-scanning category. This module
turns one external invocation into a report: an empty report on success, or a
single ``external.<name>`` error finding carrying the raw output on failure.

Per the verbatim-or-digest doctrine, a small log is preserved verbatim; a large
one is digested deterministically (head + tail + a SHA-256 of the full bytes) so
the finding never balloons yet always identifies its source output exactly.

This module only *represents* a failure as a finding — it does not decide the
gate. The caller propagates the tool's own pass/fail as the process exit code.
"""

from __future__ import annotations

import hashlib
import subprocess
from collections.abc import Mapping, Sequence
from pathlib import Path

from gmeow_tools import diagnostics
from gmeow_tools.diagnostics import DiagnosticsReport

#: Logs at or under this many bytes are kept verbatim in the finding detail;
#: larger logs are digested (head + tail + full-content hash).
DEFAULT_DETAIL_LIMIT = 4096


def _digest_detail(raw: str, limit: int) -> str:
    """Return ``raw`` verbatim when small, else a deterministic head/tail digest.

    The digest keeps the first and last halves of the budget and stamps the
    elided middle with the SHA-256 of the *full* original bytes, so two runs with
    identical output produce identical detail (determinism) while an arbitrarily
    large log stays bounded.
    """
    if len(raw.encode("utf-8")) <= limit:
        return raw
    digest = hashlib.sha256(raw.encode("utf-8")).hexdigest()
    half = max(limit // 2, 1)
    head = raw[:half]
    tail = raw[-half:]
    elided = len(raw) - len(head) - len(tail)
    return f"{head}\n... [{elided} chars elided; sha256={digest}] ...\n{tail}"


def to_diagnostics_report(
    name: str,
    argv: Sequence[str],
    returncode: int,
    stdout: str,
    stderr: str,
    *,
    detail_limit: int = DEFAULT_DETAIL_LIMIT,
) -> DiagnosticsReport:
    """Map one external-tool result into a diagnostics report (pure; no I/O).

    A zero return code yields an empty report. A non-zero code yields exactly one
    ``external.<name>`` error finding whose detail is the combined stdout/stderr
    (verbatim or digested). The message is kept stable (name + exit code only) so
    re-runs dedupe; the variable command line and log live in the detail.
    """
    tool = f"external.{name}"
    if returncode == 0:
        return diagnostics.report(tool)

    sections = [f"$ {' '.join(argv)}", f"exit code: {returncode}"]
    if stdout.strip():
        sections.append("--- stdout ---\n" + stdout.rstrip())
    if stderr.strip():
        sections.append("--- stderr ---\n" + stderr.rstrip())
    combined = "\n".join(sections)

    finding = diagnostics.finding(
        severity="error",
        code=tool,
        message=f"{name} failed (exit {returncode})",
        tool=name,
        detail=_digest_detail(combined, detail_limit),
    )
    return diagnostics.report_from_findings(tool=tool, findings=[finding])


def run_external_tool(
    name: str,
    argv: Sequence[str],
    *,
    cwd: Path | None = None,
    env: Mapping[str, str] | None = None,
    detail_limit: int = DEFAULT_DETAIL_LIMIT,
) -> DiagnosticsReport:
    """Run ``argv`` and return its result as a diagnostics report.

    Captures stdout/stderr, never raises on a non-zero exit (that is the case it
    exists to represent), and delegates the mapping to :func:`to_diagnostics_report`.
    A tool that cannot be launched at all (e.g. not installed) is itself an
    ``external.<name>`` failure, with the launch error preserved as the log.
    """
    try:
        completed = subprocess.run(
            list(argv),
            cwd=cwd,
            env=dict(env) if env is not None else None,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as exc:
        return to_diagnostics_report(
            name,
            argv,
            returncode=127,
            stdout="",
            stderr=str(exc),
            detail_limit=detail_limit,
        )
    return to_diagnostics_report(
        name,
        argv,
        returncode=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
        detail_limit=detail_limit,
    )
