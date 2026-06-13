# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for emojihash and randomart helpers."""

from __future__ import annotations

from gts.emojihash import emojihash, randomart


def test_emojihash_length_and_determinism() -> None:
    """Emojihash produces the requested number of emojis deterministically."""
    data = b"gmeow transport key"
    h1 = emojihash(data, length=8)
    h2 = emojihash(data, length=8)
    assert h1 == h2
    assert len(h1.split()) == 8


def test_emojihash_changes_with_input() -> None:
    """Different inputs produce different emoji hashes."""
    a = emojihash(b"a", length=8)
    b = emojihash(b"b", length=8)
    assert a != b


def test_emojihash_default_length() -> None:
    """Default emojihash length is 8 emojis."""
    assert len(emojihash(b"x").split()) == 8


def test_randomart_has_grid_shape() -> None:
    """Randomart produces the expected header, grid, and footer."""
    art = randomart(b"gmeow", label="ED25519")
    lines = art.splitlines()
    assert len(lines) == 11  # header + 9 rows + footer
    assert lines[0].startswith("+--[ED25519")
    assert all(line.startswith("|") and line.endswith("|") for line in lines[1:-1])


def test_randomart_changes_with_input() -> None:
    """Different inputs produce different randomart."""
    a = randomart(b"a")
    b = randomart(b"b")
    assert a != b
