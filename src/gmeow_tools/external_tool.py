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
import os
import shlex
import subprocess
from collections.abc import Mapping, Sequence
from pathlib import Path

from gmeow_tools import diagnostics
from gmeow_tools.diagnostics import DiagnosticsReport

#: Logs at or under this many characters are kept verbatim in the finding detail;
#: larger logs are digested (head + tail + full-content hash). A character budget
#: (not a byte budget) keeps the limit check consistent with the character-index
#: slicing below, so the head/tail never overlap on multi-byte text.
DEFAULT_DETAIL_LIMIT = 4096

#: Wall-clock ceiling for a wrapped tool. A hung tool must not stall a gate
#: indefinitely; on expiry the run is reported as a finding with rc 124 (the
#: conventional timeout exit code), not left to hang.
DEFAULT_TIMEOUT_SECONDS = 600.0


def _as_text(value: object) -> str:
    """Coerce a possibly-``bytes``/``None`` captured stream into text."""
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", "replace")
    return str(value)


def _digest_detail(raw: str, limit: int) -> str:
    """Return ``raw`` verbatim when small, else a deterministic head/tail digest.

    The digest keeps the first and last halves of the budget and stamps the
    elided middle with the SHA-256 of the *full* original bytes, so two runs with
    identical output produce identical detail (determinism) while an arbitrarily
    large log stays bounded. The limit is a **character** budget so it is
    consistent with the character-index slicing — with a byte budget, a
    multi-byte log whose byte-length exceeds the limit but whose char-length does
    not would yield a negative ``elided`` count and overlapping head/tail.
    """
    if len(raw) <= limit:
        return raw
    digest = hashlib.sha256(raw.encode("utf-8")).hexdigest()
    half = max(limit // 2, 1)
    head = raw[:half]
    tail = raw[-half:]
    # len(raw) > limit >= 2*half guarantees a non-negative, non-overlapping gap.
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

    sections = [f"$ {shlex.join(argv)}", f"exit code: {returncode}"]
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
    timeout: float | None = DEFAULT_TIMEOUT_SECONDS,
) -> tuple[int, DiagnosticsReport]:
    """Run ``argv`` and return ``(returncode, report)``.

    The return code is the wrapped tool's **exact** exit status, so a caller can
    mirror it faithfully (not just pass/fail). Captures stdout/stderr and never
    raises on a non-zero exit (that is the case it exists to represent), delegating
    the mapping to :func:`to_diagnostics_report`. A tool that cannot be launched
    (not installed → 127, empty argv → 127) or that hangs past ``timeout`` (→ 124)
    is itself an ``external.<name>`` failure, with the cause preserved as the log.
    A supplied ``env`` is *merged onto* the parent environment (so the child keeps
    ``PATH`` and friends, then overrides), never a full replacement.
    """
    if not argv:
        return 127, to_diagnostics_report(
            name,
            argv,
            returncode=127,
            stdout="",
            stderr="empty command list provided",
            detail_limit=detail_limit,
        )
    try:
        completed = subprocess.run(
            list(argv),
            cwd=cwd,
            env={**os.environ, **env} if env is not None else None,
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        # With text=True the captured streams are str, but the stub types them
        # loosely; coerce defensively so a bytes payload still renders.
        partial_out = _as_text(exc.stdout)
        partial_err = _as_text(exc.stderr)
        return 124, to_diagnostics_report(
            name,
            argv,
            returncode=124,
            stdout=partial_out,
            stderr=partial_err + f"\nprocess timed out after {timeout}s",
            detail_limit=detail_limit,
        )
    except OSError as exc:
        return 127, to_diagnostics_report(
            name,
            argv,
            returncode=127,
            stdout="",
            stderr=str(exc),
            detail_limit=detail_limit,
        )
    return completed.returncode, to_diagnostics_report(
        name,
        argv,
        returncode=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
        detail_limit=detail_limit,
    )
