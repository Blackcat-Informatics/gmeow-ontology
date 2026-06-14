# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Constitution-as-code gate tests (#280).

The real manifest must pass; each failure mode the gate exists to catch is
re-created in a temp manifest/constitution pair so the regressions survive
any future fix to the real data.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.constitution import (
    CONSTITUTION_FILE,
    MANIFEST_FILE,
    check_constitution,
    constitution_headings,
    load_manifest,
)

_PREFIXES = """\
@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
"""

_MINIMAL_CONSTITUTION = "## 1. Be good\n\nprose\n"


def _write_pair(
    tmp_path: Path, manifest_ttl: str, constitution_md: str
) -> tuple[Path, Path]:
    manifest = tmp_path / "constitution.ttl"
    manifest.write_text(_PREFIXES + manifest_ttl, encoding="utf-8")
    constitution = tmp_path / "CONSTITUTION.md"
    constitution.write_text(constitution_md, encoding="utf-8")
    return manifest, constitution


def test_real_manifest_passes() -> None:
    """The committed manifest, constitution, and repo agree — zero errors."""
    result = check_constitution()
    assert not result.errors, "\n".join(result.errors)


def test_every_principle_has_a_manifest_entry() -> None:
    """Bidirectional sync: heading set == manifest set, titles verbatim."""
    headings = constitution_headings(CONSTITUTION_FILE)
    manifest = load_manifest(MANIFEST_FILE)
    assert {p.number: p.title for p in manifest.principles} == headings


def test_honor_system_principles_are_visible_not_silent() -> None:
    """Practice-only principles surface as warnings (today: 1, 6, 15, 17)."""
    result = check_constitution()
    flagged = {int(w.split()[1]) for w in result.warnings if "review practice" in w}
    assert flagged == {1, 6, 15, 17}


def test_zero_enforcement_is_an_error(tmp_path: Path) -> None:
    manifest, constitution = _write_pair(
        tmp_path,
        'meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title "Be good" .\n',
        _MINIMAL_CONSTITUTION,
    )
    result = check_constitution(manifest_path=manifest, constitution_path=constitution)
    assert any("zero registered enforcement" in e for e in result.errors)


def test_stale_artifact_reference_is_an_error(tmp_path: Path) -> None:
    manifest, constitution = _write_pair(
        tmp_path,
        """\
meta:gate-x a meta:Gate ; meta:artifact "no/such/file.py" .
meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title "Be good" ;
    meta:enforcedBy meta:gate-x .
""",
        _MINIMAL_CONSTITUTION,
    )
    result = check_constitution(manifest_path=manifest, constitution_path=constitution)
    assert any("'no/such/file.py' does not exist" in e for e in result.errors)


def test_stale_symbol_make_target_and_cli_command_are_errors(tmp_path: Path) -> None:
    manifest, constitution = _write_pair(
        tmp_path,
        """\
meta:gate-x a meta:Gate ;
    meta:artifact "src/gmeow_tools/validate.py" ;
    meta:symbol "no_such_function" ;
    meta:makeTarget "no-such-target" ;
    meta:cliCommand "no-such-command" .
meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title "Be good" ;
    meta:enforcedBy meta:gate-x .
""",
        _MINIMAL_CONSTITUTION,
    )
    result = check_constitution(manifest_path=manifest, constitution_path=constitution)
    text = "\n".join(result.errors)
    assert "'no_such_function' not found" in text
    assert "Makefile target 'no-such-target'" in text
    assert "CLI command 'no-such-command'" in text


def test_orphaned_enforcement_is_an_error(tmp_path: Path) -> None:
    manifest, constitution = _write_pair(
        tmp_path,
        """\
meta:gate-used a meta:Gate ; meta:artifact "Makefile" .
meta:gate-orphan a meta:Lint ; meta:artifact "Makefile" .
meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title "Be good" ;
    meta:enforcedBy meta:gate-used .
""",
        _MINIMAL_CONSTITUTION,
    )
    result = check_constitution(manifest_path=manifest, constitution_path=constitution)
    assert any(
        "orphaned enforcement" in e and "gate-orphan" in e for e in result.errors
    )


def test_title_drift_is_an_error(tmp_path: Path) -> None:
    manifest, constitution = _write_pair(
        tmp_path,
        """\
meta:gate-x a meta:Gate ; meta:artifact "Makefile" .
meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title "Be excellent" ;
    meta:enforcedBy meta:gate-x .
""",
        _MINIMAL_CONSTITUTION,
    )
    result = check_constitution(manifest_path=manifest, constitution_path=constitution)
    assert any("title drift" in e for e in result.errors)


def test_undeclared_generator_is_an_error(tmp_path: Path) -> None:
    """A registered generator missing from the manifest fails the gate."""
    manifest, constitution = _write_pair(
        tmp_path,
        """\
meta:gate-x a meta:Gate ; meta:artifact "Makefile" ; meta:generator "gts" .
meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title "Be good" ;
    meta:enforcedBy meta:gate-x .
""",
        _MINIMAL_CONSTITUTION,
    )
    result = check_constitution(manifest_path=manifest, constitution_path=constitution)
    assert any(
        "registered but not constitutionally declared" in e for e in result.errors
    ), "\n".join(result.errors)


def test_practice_only_principle_warns_not_errors(tmp_path: Path) -> None:
    manifest, constitution = _write_pair(
        tmp_path,
        """\
meta:practice-x a meta:Practice ; meta:artifact "Makefile" .
meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title "Be good" ;
    meta:enforcedBy meta:practice-x .
""",
        _MINIMAL_CONSTITUTION,
    )
    result = check_constitution(manifest_path=manifest, constitution_path=constitution)
    assert any("only by review practice" in w for w in result.warnings)
    assert not any("zero registered enforcement" in e for e in result.errors)
