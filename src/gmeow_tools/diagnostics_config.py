# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Shared diagnostics output policy (#662).

One :class:`DiagnosticsConfig` owns *where* diagnostics go and *how* they are
projected to the console — console mode, which artifact files to write, the
output directory, filename stem, and the stable code-scanning category. It is
the single rail `gmeow-dev`, Make, and GitHub Actions all set policy through:
the same five knobs as CLI flags AND as ``GMEOW_DIAGNOSTICS_*`` environment
variables, resolved with one precedence — **flag > env > default**.

This module is pure policy: it touches no Rust, renders nothing, and never
changes a gate's exit code. ``silent`` and ``none`` suppress *output*; they never
swallow an error. Invalid tokens are a hard failure (no silent fallback), per the
no-optionality doctrine — ``auto``/``silent``/``none`` are explicitly enumerated,
validated modes, not degraded fallbacks.
"""

from __future__ import annotations

import os
import sys
from collections.abc import Mapping
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path

from gmeow_tools.config import DIST_DIR

#: The three artifact projections, in deterministic write order. Mirrors the
#: fixed order the Rust ``Report.write_artifacts`` honors regardless of selection.
ARTIFACT_KINDS: tuple[str, ...] = ("json", "sarif", "html")

DEFAULT_STEM = "gmeow-feedback"
DEFAULT_CATEGORY = "gmeow"


class ConsoleMode(StrEnum):
    """How findings are projected to the console.

    ``auto`` is resolved away during :meth:`DiagnosticsConfig.resolve` (to
    ``pretty`` on a TTY, ``text`` otherwise), so a resolved config never carries
    ``auto`` — the renderer only ever sees a concrete mode.
    """

    AUTO = "auto"
    PRETTY = "pretty"
    TEXT = "text"
    JSONL = "jsonl"
    SILENT = "silent"


def _first(*candidates: str | None) -> str:
    """Return the first non-``None`` candidate — the flag > env > default rule.

    The final candidate is always the default, so the result is never ``None``.
    """
    for candidate in candidates:
        if candidate is not None:
            return candidate
    raise AssertionError("at least one candidate (the default) must be non-None")


def _parse_artifacts(raw: str) -> frozenset[str]:
    """Parse an ``--diagnostics-artifacts`` value into a set of kinds.

    ``none`` -> empty, ``all`` -> every kind, otherwise a comma list of kinds.
    An unknown token is a hard error (a typo must not silently degrade to a
    narrower or empty selection).
    """
    token = raw.strip().lower()
    if token == "none":
        return frozenset()
    if token == "all":
        return frozenset(ARTIFACT_KINDS)
    kinds = {part.strip().lower() for part in token.split(",") if part.strip()}
    unknown = kinds - set(ARTIFACT_KINDS)
    if unknown:
        raise ValueError(
            f"unknown diagnostics artifact kind(s): {sorted(unknown)} "
            f"(expected a subset of {list(ARTIFACT_KINDS)}, or 'none'/'all')"
        )
    if not kinds:
        raise ValueError(f"empty diagnostics artifact selection: {raw!r}")
    return frozenset(kinds)


def _env_path(value: str | None) -> Path | None:
    """An env-supplied directory as a ``Path``, or ``None`` when unset/blank."""
    if value is None or not value.strip():
        return None
    return Path(value).expanduser()


@dataclass(frozen=True, slots=True)
class DiagnosticsConfig:
    """Resolved diagnostics output policy (immutable)."""

    console: ConsoleMode
    artifacts: frozenset[str]
    directory: Path
    stem: str
    category: str

    @classmethod
    def resolve(
        cls,
        *,
        console: str | None = None,
        artifacts: str | None = None,
        directory: Path | None = None,
        stem: str | None = None,
        category: str | None = None,
        env: Mapping[str, str] | None = None,
        is_tty: bool | None = None,
    ) -> DiagnosticsConfig:
        """Resolve the output policy from flags, environment, and defaults.

        Precedence is **flag > env > default** for every knob. ``auto`` resolves
        by ``is_tty`` (defaulting to ``sys.stderr.isatty()`` — the diagnostic
        console is the stderr surface, so piping stdout does not flip it). Invalid
        ``console``/``artifacts`` tokens raise rather than fall back.
        """
        env = os.environ if env is None else env
        if is_tty is None:
            is_tty = sys.stderr.isatty()

        mode = ConsoleMode(
            _first(console, env.get("GMEOW_DIAGNOSTICS_CONSOLE"), ConsoleMode.AUTO)
        )
        if mode is ConsoleMode.AUTO:
            mode = ConsoleMode.PRETTY if is_tty else ConsoleMode.TEXT

        resolved_artifacts = _parse_artifacts(
            _first(artifacts, env.get("GMEOW_DIAGNOSTICS_ARTIFACTS"), "all")
        )

        resolved_category = _first(
            category, env.get("GMEOW_DIAGNOSTICS_CATEGORY"), DEFAULT_CATEGORY
        )
        resolved_stem = _first(stem, env.get("GMEOW_DIAGNOSTICS_STEM"), DEFAULT_STEM)

        # Directory precedence: an explicit flag or env dir is used verbatim.
        # Otherwise the default is keyed on whether a *category* was explicitly
        # requested: an aggregate/manual run (no category) keeps the flat
        # ``dist/`` convention (preserving ``dist/gmeow-feedback.*``), while a
        # category run (CI per-job, ``--diagnostics-category lint``) lands under
        # ``dist/diagnostics/<category>/`` so per-job artifacts never collide.
        explicit_dir = directory or _env_path(env.get("GMEOW_DIAGNOSTICS_DIR"))
        category_explicit = category is not None or bool(
            env.get("GMEOW_DIAGNOSTICS_CATEGORY")
        )
        if explicit_dir is not None:
            resolved_dir = explicit_dir
        elif category_explicit:
            resolved_dir = DIST_DIR / "diagnostics" / resolved_category
        else:
            resolved_dir = DIST_DIR

        return cls(
            console=mode,
            artifacts=resolved_artifacts,
            directory=resolved_dir,
            stem=resolved_stem,
            category=resolved_category,
        )
