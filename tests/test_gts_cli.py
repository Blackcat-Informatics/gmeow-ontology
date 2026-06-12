# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""End-to-end tests of the ``gts`` console script (§14.1 tooling contract).

The command surface here MUST stay identical to the Rust binary's
(``crates/gts/src/bin/gts.rs`` + ``crates/gts/tests/cli.rs``): same verbs,
same refusals, same exit codes.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from gts import Term, TermKind, Writer
from gts.cli import main
from gts.wire import digest_str

CAT = "https://example.org/Cat"
LABEL = "http://www.w3.org/2000/01/rdf-schema#label"

BLOB = b"not really webp bytes"


def _blob_file(tmp_path: Path) -> tuple[Path, str]:
    w = Writer()
    w.add_terms(
        [
            Term(TermKind.IRI, CAT),
            Term(TermKind.IRI, LABEL),
            Term(TermKind.LITERAL, "Cat", lang="en"),
        ]
    )
    w.add_quads([(0, 1, 2, None)])
    w.add_blob(BLOB, mt="image/webp")
    path = tmp_path / "blob.gts"
    path.write_bytes(w.to_bytes())
    return path, digest_str(BLOB)


def test_ls_lists_digest_size_and_media_type(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    path, digest = _blob_file(tmp_path)
    assert main(["ls", str(path)]) == 0
    out = capsys.readouterr().out
    assert digest in out
    assert str(len(BLOB)) in out
    assert "image/webp" in out


def test_extract_writes_verified_bytes(tmp_path: Path) -> None:
    path, digest = _blob_file(tmp_path)
    out = tmp_path / "yak.webp"
    assert main(["extract", str(path), digest, "-o", str(out)]) == 0
    assert out.read_bytes() == BLOB


def test_extract_accepts_bare_hex_digest(tmp_path: Path) -> None:
    path, digest = _blob_file(tmp_path)
    out = tmp_path / "yak.webp"
    bare = digest.removeprefix("blake3:")
    assert main(["extract", str(path), bare, "-o", str(out)]) == 0
    assert out.read_bytes() == BLOB


def test_extract_mt_is_an_assertion_not_a_conversion(tmp_path: Path) -> None:
    path, digest = _blob_file(tmp_path)
    out = tmp_path / "yak.png"
    # asserted type mismatches the declared image/webp — refuse, never convert
    assert (
        main(["extract", str(path), digest, "-o", str(out), "--mt", "image/png"]) == 1
    )
    assert not out.exists()
    assert (
        main(["extract", str(path), digest, "-o", str(out), "--mt", "image/webp"]) == 0
    )


def test_extract_unknown_digest_fails(tmp_path: Path) -> None:
    path, _ = _blob_file(tmp_path)
    assert main(["extract", str(path), "blake3:" + "0" * 64, "-o", "/dev/null"]) == 1


def test_extract_refuses_suppressed_blob_by_default(tmp_path: Path) -> None:
    w = Writer()
    w.add_blob(BLOB, mt="image/webp")
    digest = digest_str(BLOB)
    w.add_suppress([{"kind": "blob", "digest": digest}], reason="retracted")
    path = tmp_path / "suppressed.gts"
    path.write_bytes(w.to_bytes())

    out = tmp_path / "yak.webp"
    assert main(["extract", str(path), digest, "-o", str(out)]) == 1
    assert not out.exists()
    # suppression is a display overlay (§11) — history stays extractable
    assert (
        main(["extract", str(path), digest, "-o", str(out), "--include-suppressed"])
        == 0
    )
    assert out.read_bytes() == BLOB


def test_fold_exits_nonzero_on_diagnostics() -> None:
    # damaged corpus vector: the partial fold is emitted, the exit is 1 —
    # `gts fold … && publish` pipelines must fail on damage
    damaged = Path("generated/gts-vectors/04-damaged-frame.gts")
    assert main(["fold", str(damaged)]) == 1


def _make_tree(tmp_path: Path) -> Path:
    src = tmp_path / "src"
    src.mkdir()
    (src / "a.txt").write_text("hello")
    (src / "subdir").mkdir()
    (src / "subdir" / "b.txt").write_text("world")
    return src


def test_pack_round_trips_bit_for_bit(tmp_path: Path) -> None:
    src = _make_tree(tmp_path)
    archive = tmp_path / "out.gts"
    assert main(["pack", str(src), "-o", str(archive)]) == 0

    dst = tmp_path / "dst"
    assert main(["unpack", str(archive), "-C", str(dst)]) == 0
    assert (dst / "a.txt").read_text() == "hello"
    assert (dst / "subdir" / "b.txt").read_text() == "world"

    # Identical tree produces identical archive bytes.
    archive2 = tmp_path / "out2.gts"
    assert main(["pack", str(dst), "-o", str(archive2)]) == 0
    assert archive.read_bytes() == archive2.read_bytes()


def test_pack_deduplicates_identical_content(tmp_path: Path) -> None:
    from gts.reader import read

    src = tmp_path / "src"
    src.mkdir()
    (src / "a.txt").write_text("shared")
    (src / "b.txt").write_text("shared")
    archive = tmp_path / "out.gts"
    assert main(["pack", str(src), "-o", str(archive)]) == 0
    g = read(archive.read_bytes())
    # Two FileEntries but one inline blob.
    assert len(g.blobs) == 1


def test_unpack_refuses_traversal(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    from gts import Writer

    w = Writer(profile="files")
    # Add the minimal shared terms so unpack recognises a files profile.
    w.add_terms(
        [
            Term(TermKind.IRI, "https://w3id.org/gts/files#FileEntry"),
            Term(TermKind.IRI, "https://w3id.org/gts/files#path"),
            Term(TermKind.IRI, "https://w3id.org/gts/files#digest"),
            Term(TermKind.IRI, "https://w3id.org/gts/files#size"),
            Term(TermKind.IRI, "https://w3id.org/gts/files#mode"),
            Term(TermKind.IRI, "https://w3id.org/gts/files#modified"),
            Term(TermKind.IRI, "https://w3id.org/gts/files#mediaType"),
            Term(TermKind.IRI, "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
            Term(TermKind.IRI, "http://www.w3.org/2001/XMLSchema#integer"),
            Term(TermKind.IRI, "http://www.w3.org/2001/XMLSchema#dateTime"),
        ]
    )
    # Craft a FileEntry with a traversal path and a digest, but no blob.
    # Unpack must refuse the traversal path before it complains about the
    # missing blob.
    w.add_terms(
        [
            Term(TermKind.BNODE, "e0"),
            Term(TermKind.LITERAL, "../escape.txt"),
            Term(TermKind.LITERAL, "blake3:" + "0" * 64),
        ]
    )
    w.add_quads(
        [
            (10, 7, 0, None),  # e0 a FileEntry
            (10, 1, 11, None),  # e0 path "../escape.txt"
            (10, 2, 12, None),  # e0 digest ...
        ]
    )
    archive = tmp_path / "traversal.gts"
    archive.write_bytes(w.to_bytes())

    assert main(["unpack", str(archive), "-C", str(tmp_path / "dst")]) == 1
    stderr = capsys.readouterr().err
    assert "traversal" in stderr or "escapes" in stderr, stderr


def test_diff_reports_changes(tmp_path: Path) -> None:
    src = _make_tree(tmp_path)
    archive = tmp_path / "out.gts"
    assert main(["pack", str(src), "-o", str(archive)]) == 0

    # Identical directory -> no differences.
    assert main(["diff", str(archive), str(src)]) == 0

    # Add a file.
    (src / "new.txt").write_text("new")
    assert main(["diff", str(archive), str(src)]) == 1
    # (specific reports tested via direct diff() unit tests)


def test_unpack_skips_suppressed_blob_by_default(tmp_path: Path) -> None:
    src = tmp_path / "src"
    src.mkdir()
    (src / "secret.txt").write_text("secret")
    archive = tmp_path / "out.gts"
    assert main(["pack", str(src), "-o", str(archive)]) == 0

    # Read the archive and append a suppression frame for the secret blob.
    data = bytearray(archive.read_bytes())
    from gts.wire import canonical, content_id, iter_items

    # Find the actual last frame id by walking items.
    items, _torn = iter_items(bytes(data))
    _off, last_item = items[-1]
    assert isinstance(last_item, dict)
    last_id = last_item["id"]
    frame = {
        "t": "suppress",
        "prev": last_id,
        "d": {"targets": [{"kind": "blob", "digest": digest_str(b"secret")}]},
    }
    frame["id"] = content_id(frame)
    data += canonical(frame)
    archive.write_bytes(data)

    dst = tmp_path / "dst"
    assert main(["unpack", str(archive), "-C", str(dst)]) == 0
    assert not (dst / "secret.txt").exists()
    assert main(["unpack", str(archive), "-C", str(dst), "--include-suppressed"]) == 0
    assert (dst / "secret.txt").read_text() == "secret"
