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
