# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""slice fix-deps — propose manifest dependency edits as a reviewable patch.

Computes undeclared/stale dependencies by invoking the native ownership
analyzer (the authoritative ``gmeow_slice`` PyO3 binding — the native
``SliceCatalog`` + ``OwnershipAnalyzer``, #820 S8) and emits a unified diff
against each affected ``slices/*/manifest.ttl``.  By default the patch is
printed to stdout; nothing is written.  Pass ``--apply`` to apply the changes
in-place.

Two-pass contract (RFC #820 §11 / S7):
1. The ownership analyzer reads authored manifests (immutable input).
2. This module proposes edits to those manifests based on the computed edges.
   It NEVER writes gmeow:computedSliceDependency or any analysis-graph triple
   into a manifest — only gmeow:sliceDependsOn additions/removals.
"""

from __future__ import annotations

import difflib
import re
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    pass

# ── Constants ──────────────────────────────────────────────────────────────────

GMEOW_NS = "https://blackcatinformatics.ca/gmeow/"
SLICE_DEPENDS_ON_PRED = "gmeow:sliceDependsOn"
_PREFIX_RE = re.compile(r"@prefix\s+gmeow:\s*<([^>]+)>\s*\.")


def _gmeow_prefix_iri(text: str) -> str:
    """Extract the gmeow: prefix IRI from Turtle text."""
    m = _PREFIX_RE.search(text)
    return m.group(1) if m else GMEOW_NS


# ── Dependency proposal ────────────────────────────────────────────────────────


class DepProposal:
    """Proposed additions and removals for a single manifest.ttl."""

    def __init__(self, manifest_path: Path) -> None:
        """Initialise a proposal for the given manifest path."""
        self.manifest_path = manifest_path
        self.to_add: list[str] = []  # IRIs to add as sliceDependsOn
        self.to_remove: list[str] = []  # IRIs to remove from sliceDependsOn

    def is_empty(self) -> bool:
        """Return True if there are no additions or removals to propose."""
        return not self.to_add and not self.to_remove


def _shorten_iri(iri: str, gmeow_ns: str) -> str:
    """Shorten a full IRI to a gmeow:-prefixed form if possible."""
    if iri.startswith(gmeow_ns):
        return f"gmeow:{iri[len(gmeow_ns) :]}"
    return f"<{iri}>"


# ── Manifest patching ─────────────────────────────────────────────────────────

_DEPENDS_ON_LINE_RE = re.compile(
    r"""^\s*gmeow:sliceDependsOn\s+(?P<ref>\S+)\s*[;.]?\s*$"""
)


def _apply_proposal_to_text(original: str, proposal: DepProposal) -> str:
    """Return the patched Turtle text for a manifest, or the original if unchanged.

    Strategy:
    - Removals: delete lines that match ``gmeow:sliceDependsOn <target>`` or
      ``gmeow:sliceDependsOn gmeow:<local>`` for each removed IRI.
    - Additions: insert new ``gmeow:sliceDependsOn gmeow:<local>`` lines after
      the last existing sliceDependsOn line (or after the ``a gmeow:Slice ;``
      line if none exist).

    This is a best-effort text transform, not a round-trip RDF serialization,
    so the output preserves the author's formatting for unchanged lines.
    """
    gmeow_ns = _gmeow_prefix_iri(original)
    lines = original.splitlines(keepends=True)

    # ── Removals ────────────────────────────────────────────────────────────
    remove_set = {_shorten_iri(iri, gmeow_ns) for iri in proposal.to_remove}
    remove_set |= {f"<{iri}>" for iri in proposal.to_remove}

    kept_lines: list[str] = []
    for line in lines:
        m = _DEPENDS_ON_LINE_RE.match(line)
        if m and m.group("ref") in remove_set:
            continue
        kept_lines.append(line)

    # ── Additions ────────────────────────────────────────────────────────────
    if proposal.to_add:
        new_lines = [
            f"    {SLICE_DEPENDS_ON_PRED} {_shorten_iri(iri, gmeow_ns)} ;\n"
            for iri in sorted(proposal.to_add)
        ]
        # Find the last sliceDependsOn line, or the "a gmeow:Slice" line.
        insert_after = -1
        for i, line in enumerate(kept_lines):
            if "sliceDependsOn" in line or "a gmeow:Slice" in line:
                insert_after = i
        if insert_after >= 0:
            kept_lines[insert_after + 1 : insert_after + 1] = new_lines
        else:
            # Fallback: prepend before the first non-prefix, non-blank line.
            for i, line in enumerate(kept_lines):
                stripped = line.strip()
                if stripped and not stripped.startswith("@prefix"):
                    kept_lines[i + 1 : i + 1] = new_lines
                    break

    return "".join(kept_lines)


# ── Unified diff ──────────────────────────────────────────────────────────────


def _make_diff(original: str, patched: str, path: Path) -> str:
    """Return a unified diff string (empty if unchanged)."""
    orig_lines = original.splitlines(keepends=True)
    patched_lines = patched.splitlines(keepends=True)
    label = str(path)
    diff_lines = list(
        difflib.unified_diff(
            orig_lines,
            patched_lines,
            fromfile=f"a/{label}",
            tofile=f"b/{label}",
        )
    )
    return "".join(diff_lines)


# ── Main entry point ──────────────────────────────────────────────────────────


def compute_fix_deps(
    slices_root: Path,
    *,
    apply: bool = False,
) -> list[str]:
    """Compute proposed manifest dependency edits and return a list of diff strings.

    When ``apply=True``, also write the patched files in-place.

    Hard-fails (raises ``RuntimeError``) if:
    - the native analyzer cannot be invoked,
    - a manifest cannot be read.

    Never writes computed analysis triples into manifests.
    """
    # Lazy import the native Rust extension (the unified gmeow_native cdylib's
    # `slice` submodule, aliased via the gmeow_slice shim — #820 S8).
    try:
        import gmeow_slice
    except ModuleNotFoundError as exc:
        raise RuntimeError(
            "gmeow-slice native extension not found; "
            "run `make native-py` to build and install the unified extension"
        ) from exc

    # Discover slices using the native catalog.
    try:
        catalog = gmeow_slice.SliceCatalog.discover(str(slices_root))
    except Exception as exc:
        raise RuntimeError(f"slice catalog discovery failed: {exc}") from exc

    # Run the native ownership analyzer.
    try:
        report = gmeow_slice.OwnershipAnalyzer(catalog).analyze()
    except Exception as exc:
        raise RuntimeError(f"ownership analysis failed: {exc}") from exc

    # Build proposals from the computed edges.
    # `report.edges` is a list of DependencyEdge objects.
    proposals: dict[str, DepProposal] = {}

    # Locate manifest.ttl paths per slice IRI.
    slice_manifest_paths: dict[str, Path] = {}
    for record in catalog.records():
        iri = record.manifest.slice_iri
        for artifact in record.artifacts:
            if artifact.role == "Manifest":
                # The artifact's logical_path is relative to the slice dir.
                # We need to find the actual path by discovering the slice dir.
                # The catalog knows the slice root; ask it via the record's
                # directory attribute if available.
                manifest_path = _find_manifest(slices_root, iri, artifact.logical_path)
                if manifest_path is not None:
                    slice_manifest_paths[iri] = manifest_path
                break

    for edge in report.edges:
        # Only propose for undeclared (add) and stale (remove) semantic edges.
        if edge.reconciliation not in ("undeclared", "stale"):
            continue
        if not edge.is_semantic:
            continue

        from_iri = edge.from_slice
        to_iri = edge.to_slice

        if from_iri not in proposals:
            manifest_path = slice_manifest_paths.get(from_iri)
            if manifest_path is None:
                continue
            proposals[from_iri] = DepProposal(manifest_path)

        if edge.reconciliation == "undeclared":
            proposals[from_iri].to_add.append(to_iri)
        elif edge.reconciliation == "stale":
            proposals[from_iri].to_remove.append(to_iri)

    # Produce diffs (and optionally apply).
    diffs: list[str] = []
    for _slice_iri, proposal in sorted(proposals.items()):
        if proposal.is_empty():
            continue
        try:
            original = proposal.manifest_path.read_text(encoding="utf-8")
        except OSError as exc:
            raise RuntimeError(f"cannot read {proposal.manifest_path}: {exc}") from exc

        patched = _apply_proposal_to_text(original, proposal)
        diff = _make_diff(original, patched, proposal.manifest_path)
        if diff:
            diffs.append(diff)
            if apply:
                proposal.manifest_path.write_text(patched, encoding="utf-8")

    return diffs


def _find_manifest(slices_root: Path, slice_iri: str, logical_path: str) -> Path | None:
    """Attempt to locate a manifest.ttl by scanning slices_root for the IRI.

    This is a best-effort heuristic: scan all manifest.ttl files under
    slices_root to find the one that declares the given slice IRI.
    """
    # Use the local name as a hint for the slice directory name.
    local = slice_iri.rstrip("/#").rsplit("/", 1)[-1]
    # Fast path: try slices/<anything>/<local>/manifest.ttl.
    candidates = list(slices_root.rglob(f"{local}/manifest.ttl"))
    for candidate in candidates:
        try:
            text = candidate.read_text(encoding="utf-8")
            if slice_iri in text or local in text:
                return candidate
        except OSError:
            continue
    # Slow path: scan all manifest.ttl files.
    for candidate in slices_root.rglob("manifest.ttl"):
        try:
            if slice_iri in candidate.read_text(encoding="utf-8"):
                return candidate
        except OSError:
            continue
    return None
