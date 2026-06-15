# SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Regression tests for the generated bundle merge driver (#532)."""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

from gmeow_tools.config import PROJECT_ROOT


def _git(
    repo: Path, *args: str, check: bool = True
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=repo,
        check=check,
        text=True,
        capture_output=True,
    )


def test_bootstrap_configures_ours_merge_driver(tmp_path: Path) -> None:
    """The install bootstrap makes Git's custom ``ours`` driver available locally."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _git(repo, "init", "-b", "main")

    script = PROJECT_ROOT / "scripts" / "bootstrap-git-merge-drivers.sh"
    subprocess.run(["bash", str(script)], cwd=repo, check=True)

    driver = _git(repo, "config", "--local", "--get", "merge.ours.driver").stdout
    name = _git(repo, "config", "--local", "--get", "merge.ours.name").stdout
    assert driver.strip() == "true"
    assert "generated binary artifacts" in name


def test_generated_bundle_merge_keeps_current_side(tmp_path: Path) -> None:
    """Conflicting edits to generated/dist/gmeow.gts auto-resolve to our side."""
    repo = tmp_path / "repo"
    repo.mkdir()
    _git(repo, "init", "-b", "main")
    _git(repo, "config", "user.email", "agent@example.invalid")
    _git(repo, "config", "user.name", "GMEOW Agent")

    shutil.copyfile(PROJECT_ROOT / ".gitattributes", repo / ".gitattributes")
    bundle = repo / "generated" / "dist" / "gmeow.gts"
    bundle.parent.mkdir(parents=True)
    bundle.write_bytes(b"base")
    _git(repo, "add", ".")
    _git(repo, "commit", "-m", "base")

    script = PROJECT_ROOT / "scripts" / "bootstrap-git-merge-drivers.sh"
    subprocess.run(["bash", str(script)], cwd=repo, check=True)

    _git(repo, "switch", "-c", "side")
    bundle.write_bytes(b"side")
    _git(repo, "commit", "-am", "side")

    _git(repo, "switch", "main")
    bundle.write_bytes(b"main")
    _git(repo, "commit", "-am", "main")

    merge = _git(repo, "merge", "side")
    assert merge.returncode == 0
    assert bundle.read_bytes() == b"main"
    assert _git(repo, "status", "--porcelain").stdout == ""
