# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Files-profile pack/unpack/diff logic for GTS archives.

Implements the ``files`` profile defined in ``docs/GTS-SPEC.md`` §13.2 and
§14.2: a portable, content-addressed archive of a file tree with deterministic,
reproducible packing.
"""

from __future__ import annotations

import contextlib
import mimetypes
import os
import stat
from collections.abc import Iterable
from datetime import UTC, datetime
from pathlib import Path

from gts.model import Graph, Term, TermKind
from gts.wire import digest_str
from gts.writer import Writer

FILES_NS = "https://w3id.org/gts/files#"
RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
XSD_INTEGER = "http://www.w3.org/2001/XMLSchema#integer"
XSD_DATETIME = "http://www.w3.org/2001/XMLSchema#dateTime"


def _iri(value: str) -> Term:
    return Term(TermKind.IRI, value)


def _literal(
    value: str,
    *,
    datatype: int | None = None,
    lang: str | None = None,
) -> Term:
    return Term(TermKind.LITERAL, value, datatype=datatype, lang=lang)


def _bnode(label: str) -> Term:
    return Term(TermKind.BNODE, label)


def _parse_rfc3339_seconds(text: str) -> datetime:
    """Parse a RFC 3339 timestamp into a UTC datetime.

    Handles the ``Z`` suffix and comma-separated fractional seconds without
    adding a new dependency.
    """
    text = text.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    text = text.replace(",", ".")
    return datetime.fromisoformat(text)


def _safe_archive_path(name: str) -> None:
    """Refuse paths that could escape an archive or are ill-formed."""
    if not name:
        msg = "empty archive path"
        raise ValueError(msg)
    if name.startswith("/"):
        msg = f"absolute path not allowed in archive: {name}"
        raise ValueError(msg)
    for part in name.split("/"):
        if part == "..":
            msg = f"path traversal not allowed in archive: {name}"
            raise ValueError(msg)


def _resolve_sources(sources: Iterable[Path]) -> list[tuple[Path, str]]:
    """Map each source file to its archive-relative path.

    Returns a sorted list of (filesystem path, archive path) tuples. Files are
    added under their basename; directories are walked recursively.
    """
    entries: list[tuple[Path, str]] = []
    for src in sources:
        src = src.resolve()
        if not src.exists():
            msg = f"source does not exist: {src}"
            raise FileNotFoundError(msg)
        if src.is_file():
            name = src.name
            _safe_archive_path(name)
            entries.append((src, name))
        elif src.is_dir():
            for root, _dirs, files in os.walk(src):
                for fname in files:
                    fspath = Path(root) / fname
                    relpath = fspath.relative_to(src).as_posix()
                    _safe_archive_path(relpath)
                    entries.append((fspath, relpath))
        else:
            msg = f"unsupported source type: {src}"
            raise ValueError(msg)
    # Deterministic ordering: lexicographic by archive path.
    entries.sort(key=lambda e: e[1])
    return entries


def _dt_literal(dt: datetime, xsd_datetime_id: int) -> Term:
    """Serialise a UTC datetime as xsd:dateTime with second precision."""
    text = dt.strftime("%Y-%m-%dT%H:%M:%SZ")
    return _literal(text, datatype=xsd_datetime_id)


def _mode_literal(mode: int, xsd_integer_id: int) -> Term:
    return _literal(str(mode), datatype=xsd_integer_id)


def _size_literal(size: int, xsd_integer_id: int) -> Term:
    return _literal(str(size), datatype=xsd_integer_id)


def pack(
    sources: Iterable[Path],
    *,
    external_over: int | None = None,
) -> bytes:
    """Pack files/directories into a deterministic GTS files-profile archive."""
    w = Writer(profile="files")

    # Shared vocabulary terms.
    shared = [
        _iri(FILES_NS + "FileEntry"),
        _iri(FILES_NS + "path"),
        _iri(FILES_NS + "digest"),
        _iri(FILES_NS + "size"),
        _iri(FILES_NS + "mode"),
        _iri(FILES_NS + "modified"),
        _iri(FILES_NS + "mediaType"),
        _iri(RDF_TYPE),
        _iri(XSD_INTEGER),
        _iri(XSD_DATETIME),
    ]
    w.add_terms(shared)
    file_entry_id = 0
    path_id = 1
    digest_id = 2
    size_id = 3
    mode_id = 4
    modified_id = 5
    media_type_id = 6
    type_id = 7
    xsd_integer_id = 8
    xsd_datetime_id = 9

    entries = _resolve_sources(sources)

    # Build per-file terms and quads. We emit all terms first, then all quads,
    # then all blobs — the catalog-before-payload delivery schedule (§3.2).
    file_terms: list[Term] = []
    quads: list[tuple[int, int, int, int | None]] = []
    blobs: list[tuple[bytes, str, str]] = []  # (bytes, digest, media_type)
    for idx, (fspath, relpath) in enumerate(entries):
        data = fspath.read_bytes()
        digest = digest_str(data)
        st = fspath.stat()
        size = st.st_size
        mode = stat.S_IMODE(st.st_mode)
        mtime = datetime.fromtimestamp(st.st_mtime, tz=UTC)
        mt, _ = mimetypes.guess_type(str(fspath))
        mt = mt or "application/octet-stream"

        entry_label = f"f{idx}"
        entry_term = _bnode(entry_label)
        path_term = _literal(relpath)
        digest_term = _literal(digest)
        size_term = _size_literal(size, xsd_integer_id)
        mode_term = _mode_literal(mode, xsd_integer_id)
        modified_term = _dt_literal(mtime, xsd_datetime_id)
        media_term = _literal(mt)

        base = len(shared) + len(file_terms)
        file_terms.extend(
            [
                entry_term,
                path_term,
                digest_term,
                size_term,
                mode_term,
                modified_term,
                media_term,
            ]
        )
        entry_id = base
        quads.append((entry_id, type_id, file_entry_id, None))
        quads.append((entry_id, path_id, base + 1, None))
        quads.append((entry_id, digest_id, base + 2, None))
        quads.append((entry_id, size_id, base + 3, None))
        quads.append((entry_id, mode_id, base + 4, None))
        quads.append((entry_id, modified_id, base + 5, None))
        quads.append((entry_id, media_type_id, base + 6, None))
        blobs.append((data, digest, mt))

    if file_terms:
        w.add_terms(file_terms)
    if quads:
        w.add_quads(quads)

    # Emit blob frames after the catalog. Deduplicate by digest.
    seen: set[str] = set()
    for data, digest, mt in blobs:
        if digest in seen:
            continue
        seen.add(digest)
        if external_over is not None and len(data) > external_over:
            # External blob: register by digest only.
            w.add_frame(
                "blob",
                payload=None,
                raw=None,
                pub={"digest": digest, "mt": mt},
            )
        else:
            w.add_blob(data, mt=mt)

    return bytes(w.to_bytes())


def _read_file_entries(graph: Graph) -> dict[str, dict[str, object]]:
    """Read FileEntry quads from a folded graph.

    Returns a mapping from archive path to a dict of field values.
    """
    # Find the shared term ids.
    files_ns = FILES_NS
    type_id = None
    file_entry_id = None
    field_ids: dict[str, int] = {}
    for idx, term in enumerate(graph.terms):
        if term.kind is not TermKind.IRI or term.value is None:
            continue
        term_value = term.value
        if term_value == RDF_TYPE:
            type_id = idx
        elif term_value == files_ns + "FileEntry":
            file_entry_id = idx
        elif term_value.startswith(files_ns):
            field_name = term_value[len(files_ns) :]
            field_ids[field_name] = idx

    if type_id is None or file_entry_id is None:
        msg = "not a files-profile archive"
        raise ValueError(msg)

    entries: dict[int, dict[str, object]] = {}
    file_entry_subjects: set[int] = set()
    for s, p, o, _g in graph.quads:
        if p == type_id and o == file_entry_id:
            file_entry_subjects.add(s)
            entries.setdefault(s, {"_id": s})
        elif p in field_ids.values():
            field_name = next(k for k, v in field_ids.items() if v == p)
            term = graph.terms[o]
            value: object
            if (term.kind is TermKind.LITERAL and term.value is not None) or (
                term.kind is TermKind.IRI and term.value is not None
            ):
                value = term.value
            else:
                value = ""
            entries.setdefault(s, {"_id": s})[field_name] = value

    by_path: dict[str, dict[str, object]] = {}
    for s, entry in entries.items():
        if s not in file_entry_subjects:
            continue
        path = entry.get("path")
        if not isinstance(path, str):
            continue
        by_path[path] = entry
    return by_path


def _dest_path(dest: Path, archive_path: str) -> Path:
    """Resolve an archive path under dest, refusing traversal."""
    if archive_path.startswith("/"):
        msg = f"absolute path in archive: {archive_path}"
        raise ValueError(msg)
    for part in archive_path.split("/"):
        if part == "..":
            msg = f"path traversal in archive: {archive_path}"
            raise ValueError(msg)
    target = (dest / archive_path).resolve()
    dest_resolved = dest.resolve()
    if not target.is_relative_to(dest_resolved):
        msg = f"path escapes destination: {archive_path}"
        raise ValueError(msg)
    return target


def _suppressed_blob_digests(graph: Graph) -> set[str]:
    """Digests hidden by ``{"kind": "blob", "digest": ...}`` targets (§11)."""
    out: set[str] = set()
    for sup in graph.suppressions:
        for target in sup.targets:
            if target.get("kind") != "blob":
                continue
            d = target.get("digest")
            if isinstance(d, bytes):
                out.add(f"blake3:{d.hex()}")
            elif isinstance(d, str):
                out.add(d if d.startswith("blake3:") else f"blake3:{d}")
    return out


def unpack(
    graph: Graph,
    dest: Path,
    *,
    include_suppressed: bool = False,
) -> None:
    """Extract FileEntry quads from a folded graph into dest."""
    entries = _read_file_entries(graph)
    suppressed = _suppressed_blob_digests(graph) if not include_suppressed else set()
    dest.mkdir(parents=True, exist_ok=True)

    for path, entry in entries.items():
        # Refuse traversal before touching any blob.
        target = _dest_path(dest, path)

        digest = entry.get("digest")
        if not isinstance(digest, str):
            continue
        if digest in suppressed:
            continue
        data = graph.blobs.get(digest)
        if data is None:
            msg = f"missing inline blob for {path}: {digest}"
            raise ValueError(msg)
        if digest_str(data) != digest:
            msg = f"integrity failure for {path}: {digest}"
            raise ValueError(msg)

        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)

        mode = entry.get("mode")
        if isinstance(mode, str):
            with contextlib.suppress(OSError, ValueError):
                target.chmod(int(mode))

        modified = entry.get("modified")
        if isinstance(modified, str):
            with contextlib.suppress(OSError, ValueError):
                dt = _parse_rfc3339_seconds(modified)
                ts = dt.timestamp()
                os.utime(target, (ts, ts))


def diff(graph: Graph, directory: Path) -> list[str]:
    """Compare an archive to a directory by content digest.

    Returns a list of human-readable change lines:
      - added: <path>
      - removed: <path>
      - modified: <path>
    """
    entries = _read_file_entries(graph)
    archive_digests = {p: e.get("digest") for p, e in entries.items()}

    disk_digests: dict[str, str] = {}
    if directory.exists():
        for root, _dirs, files in os.walk(directory):
            for fname in files:
                fspath = Path(root) / fname
                relpath = fspath.relative_to(directory).as_posix()
                disk_digests[relpath] = digest_str(fspath.read_bytes())

    archive_paths = set(archive_digests.keys())
    disk_paths = set(disk_digests.keys())

    lines: list[str] = []
    for path in sorted(archive_paths - disk_paths):
        lines.append(f"removed: {path}")
    for path in sorted(disk_paths - archive_paths):
        lines.append(f"added: {path}")
    for path in sorted(archive_paths & disk_paths):
        if archive_digests.get(path) != disk_digests.get(path):
            lines.append(f"modified: {path}")
    return lines
