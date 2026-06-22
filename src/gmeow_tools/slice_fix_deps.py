# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""slice fix-deps — propose manifest dependency edits as a reviewable patch.

All the real work lives in the native ``gmeow_slice`` crate (#820 G8, RUST-FIRST):
the native catalog discovers slices and retains each one's on-disk directory, the
ownership analyzer computes undeclared/stale ``gmeow:sliceDependsOn`` edges, and
``SliceCatalog.fix_deps()`` produces, per affected manifest, the (path, original,
patched) text via an RDF-aware, surgical, re-parse-validated edit. There is no
Python-side manifest scanning, substring matching, or line-regex Turtle surgery
anymore — those were the HIGH-8 (wrong-manifest) and HIGH-7 (malformed-Turtle)
bugs the reviewers flagged.

This module is now a thin surface: it invokes the native function and formats a
unified diff for display. With ``--apply`` it writes the native ``patched_text``
in place.

Two-pass contract (RFC #820 §11 / S7): the ownership analyzer reads authored
manifests (immutable input); fix-deps only ever proposes ``gmeow:sliceDependsOn``
additions/removals — never ``gmeow:computedSliceDependency`` or any other
analysis-graph triple.
"""

from __future__ import annotations

import difflib
from pathlib import Path

# ── Unified diff ──────────────────────────────────────────────────────────────


def _make_diff(original: str, patched: str, path: str) -> str:
    """Return a unified diff string (empty if unchanged)."""
    diff_lines = difflib.unified_diff(
        original.splitlines(keepends=True),
        patched.splitlines(keepends=True),
        fromfile=f"a/{path}",
        tofile=f"b/{path}",
    )
    return "".join(diff_lines)


# ── Main entry point ──────────────────────────────────────────────────────────


def compute_fix_deps(
    slices_root: Path,
    *,
    apply: bool = False,
) -> list[str]:
    """Compute proposed manifest dependency edits and return a list of diff strings.

    When ``apply=True``, also write the native ``patched_text`` for each affected
    manifest in-place.

    Hard-fails (raises ``RuntimeError``) if the native extension is unavailable,
    catalog discovery fails, or the native patcher hard-fails (a manifest cannot
    be read/parsed, or a patched manifest fails its post-edit re-parse check).

    Never writes computed analysis triples into manifests — only the native
    ``gmeow:sliceDependsOn`` additions/removals.
    """
    try:
        import gmeow_slice
    except ModuleNotFoundError as exc:
        raise RuntimeError(
            "gmeow-slice native extension not found; "
            "run `make native-py` to build and install the unified extension"
        ) from exc

    try:
        catalog = gmeow_slice.SliceCatalog.discover(str(slices_root))
    except Exception as exc:
        raise RuntimeError(f"slice catalog discovery failed: {exc}") from exc

    try:
        patches = catalog.fix_deps()
    except Exception as exc:
        raise RuntimeError(f"slice fix-deps failed: {exc}") from exc

    diffs: list[str] = []
    for patch in patches:
        diff = _make_diff(patch.original_text, patch.patched_text, patch.manifest_path)
        if not diff:
            continue
        diffs.append(diff)
        if apply:
            Path(patch.manifest_path).write_text(patch.patched_text, encoding="utf-8")

    return diffs
