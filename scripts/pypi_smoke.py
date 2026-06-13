# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

"""Post-publish smoke test for the ``gmeow`` PyPI package.

Creates a fresh virtual environment, installs ``gmeow`` from PyPI, runs the
README quickstart using a ``.gts`` file and the ``gts`` CLI binary, and asserts
the whole sequence completes within five minutes (Principle 13).
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

MAX_ELAPSED_SECONDS = 300


def _venv_python(venv: Path) -> Path:
    """Return the Python executable inside a freshly created venv."""
    if sys.platform == "win32":
        return venv / "Scripts" / "python.exe"
    return venv / "bin" / "python"


def _venv_bin(venv: Path) -> Path:
    """Return the directory containing installed console scripts."""
    if sys.platform == "win32":
        return venv / "Scripts"
    return venv / "bin"


def _run(
    args: list[str],
    *,
    env: dict[str, str] | None = None,
    timeout: int = 120,
) -> None:
    """Run a command with the inherited environment plus optional overrides."""
    subprocess.run(args, check=True, env=env, timeout=timeout)


def main() -> int:
    """Install ``gmeow`` from PyPI and run the five-minute quickstart."""
    start = time.monotonic()

    with tempfile.TemporaryDirectory() as tmpdir:
        venv = Path(tmpdir) / "smoke-venv"
        data = Path(tmpdir) / "data"
        data.mkdir()
        assistant = data / "assistant.gts"

        python = _venv_python(venv)
        bin_dir = _venv_bin(venv)

        # 1. Create a clean virtual environment.
        _run([sys.executable, "-m", "venv", str(venv)])

        # 2. Install the published ``gmeow`` client from PyPI.
        _run(
            [str(python), "-m", "pip", "install", "--upgrade", "pip", "gmeow"],
            timeout=180,
        )

        # 3. Run the README quickstart using a ``.gts`` file.
        quickstart = f'''
from gmeow import Memory

mem = Memory("{assistant}")
claim = mem.store(
    "Patrick prefers explicit error handling over exceptions-as-flow",
    source="pypi smoke test",
    confidence=0.8,
    according_to="pypi-smoke",
)
assert mem.recall("error handling preferences", min_confidence=0.5)
mem.revise(
    claim,
    reason="user stated the opposite for scripts",
    superseded_by=mem.store(
        "For one-off scripts Patrick is fine with exceptions-as-flow",
        confidence=0.9,
        according_to="pypi-smoke",
    ),
)
print("quickstart-ok")
'''
        _run([str(python), "-c", quickstart])

        # 4. Verify the engine CLI binary is still named ``gts``.
        env = os.environ.copy()
        env["PATH"] = f"{bin_dir}{os.pathsep}{env['PATH']}"
        _run([str(bin_dir / "gts"), "info", str(assistant)], env=env)

    elapsed = time.monotonic() - start
    print(f"smoke-completed in {elapsed:.1f}s")

    if elapsed > MAX_ELAPSED_SECONDS:
        print(
            f"FAIL: smoke took {elapsed:.1f}s, limit {MAX_ELAPSED_SECONDS}s",
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
