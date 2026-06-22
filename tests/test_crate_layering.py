# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Crate-layering gate (#820 S0): kernel purity + an acyclic crate DAG.

Covers the live workspace (the real ``crates/*/Cargo.toml`` graph must pass),
plus synthetic fixtures for each hard failure: a kernel that grows a first-party
dependency, a dependency cycle, a dangling path edge, and the exclusion of a
path-less registry crate (``gmeow-gts``) from the first-party graph.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.crate_layering import (
    KERNEL_CRATE,
    check_crate_layering,
    findings_to_result,
    to_diagnostics_report,
)


def _write_crate(
    crates_dir: Path,
    name: str,
    deps: dict[str, str],
    *,
    registry: dict[str, str] | None = None,
) -> None:
    """Write a minimal ``crates/<dir>/Cargo.toml`` with the given dependencies.

    ``deps`` maps a dependency crate name to the *directory* it lives in (a path
    dep, so first-party); ``registry`` maps a dependency name to a version (a
    path-less registry dep, so NOT first-party).
    """
    crate_dir = crates_dir / name
    crate_dir.mkdir(parents=True)
    lines = ["[package]", f'name = "{name}"', 'version = "0.1.0"', "", "[dependencies]"]
    for dep_name, dep_dir in deps.items():
        lines.append(f'{dep_name} = {{ path = "../{dep_dir}" }}')
    for dep_name, version in (registry or {}).items():
        lines.append(f'{dep_name} = "{version}"')
    (crate_dir / "Cargo.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")


def test_live_workspace_passes() -> None:
    """The committed crate graph must pass: kernel pure, DAG acyclic."""
    report = check_crate_layering(PROJECT_ROOT / "crates")
    assert report.ok, report.errors
    # The kernel is present and carries zero first-party edges.
    assert report.edges.get(KERNEL_CRATE) == set()


def test_kernel_must_be_present(tmp_path: Path) -> None:
    """A workspace without the kernel crate fails."""
    crates = tmp_path / "crates"
    _write_crate(crates, "gmeow-other", {})
    report = check_crate_layering(crates)
    assert not report.ok
    assert any("not found" in e for e in report.errors)


def test_kernel_impurity_fails(tmp_path: Path) -> None:
    """A first-party dependency added to the kernel is a hard error."""
    crates = tmp_path / "crates"
    _write_crate(crates, "gmeow-diagnostics", {})
    # gmeow-rdf must NOT depend on a first-party crate.
    _write_crate(crates, "gmeow-rdf", {"gmeow-diagnostics": "gmeow-diagnostics"})
    report = check_crate_layering(crates)
    assert not report.ok
    assert any("ZERO first-party" in e for e in report.errors)


def test_registry_dep_is_not_first_party(tmp_path: Path) -> None:
    """A path-less ``gmeow-*`` registry dep (gmeow-gts) is an external boundary.

    The kernel may carry it without breaking purity, and it never appears as a
    first-party edge.
    """
    crates = tmp_path / "crates"
    _write_crate(crates, "gmeow-rdf", {}, registry={"gmeow-gts": "0.9.4"})
    report = check_crate_layering(crates)
    assert report.ok, report.errors
    assert report.edges["gmeow-rdf"] == set()


def test_cycle_is_detected(tmp_path: Path) -> None:
    """A first-party dependency cycle is a hard error naming the loop."""
    crates = tmp_path / "crates"
    _write_crate(crates, "gmeow-rdf", {})
    _write_crate(crates, "gmeow-a", {"gmeow-b": "gmeow-b"})
    _write_crate(crates, "gmeow-b", {"gmeow-a": "gmeow-a"})
    report = check_crate_layering(crates)
    assert not report.ok
    cycle_errors = [e for e in report.errors if "cycle" in e]
    assert cycle_errors
    assert "gmeow-a" in cycle_errors[0] and "gmeow-b" in cycle_errors[0]


def test_dangling_path_edge_fails(tmp_path: Path) -> None:
    """A first-party path dep that resolves to no crate is a hard error."""
    crates = tmp_path / "crates"
    _write_crate(crates, "gmeow-rdf", {})
    _write_crate(crates, "gmeow-a", {"gmeow-missing": "gmeow-missing"})
    report = check_crate_layering(crates)
    assert not report.ok
    assert any("not a crates/* member" in e for e in report.errors)


def test_renamed_package_path_dep_is_first_party(tmp_path: Path) -> None:
    """A first-party path dep hidden behind a ``package = "gmeow-..."`` rename is
    still a real layering edge — a kernel that pulls it in is impure (#820 S0).

    The table key (``aliased``) does NOT start with ``gmeow-``; only the renamed
    ``package`` reveals the real crate. Keying on the table key alone would let a
    kernel-purity / cycle bypass slip through.
    """
    crates = tmp_path / "crates"
    crates.mkdir(parents=True)
    _write_crate(crates, "gmeow-diagnostics", {})
    kernel_dir = crates / "gmeow-rdf"
    kernel_dir.mkdir()
    renamed_dep = (
        'aliased = { path = "../gmeow-diagnostics", package = "gmeow-diagnostics" }'
    )
    (kernel_dir / "Cargo.toml").write_text(
        "\n".join(
            [
                "[package]",
                'name = "gmeow-rdf"',
                'version = "0.1.0"',
                "",
                "[dependencies]",
                renamed_dep,
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    report = check_crate_layering(crates)
    # The renamed edge is resolved to the real crate and the kernel is impure.
    assert not report.ok
    assert any("ZERO first-party" in e for e in report.errors)
    assert report.edges["gmeow-rdf"] == {"gmeow-diagnostics"}


def test_diagnostics_projection_carries_errors(tmp_path: Path) -> None:
    """The diagnostics projection emits one finding per violation."""
    crates = tmp_path / "crates"
    _write_crate(crates, "gmeow-diagnostics", {})
    _write_crate(crates, "gmeow-rdf", {"gmeow-diagnostics": "gmeow-diagnostics"})
    report = check_crate_layering(crates)
    result = findings_to_result(report)
    assert result.errors == report.errors
    dreport = to_diagnostics_report(report)
    codes = {item["code"] for item in dreport.findings}
    assert "crate-layering.violation" in codes
