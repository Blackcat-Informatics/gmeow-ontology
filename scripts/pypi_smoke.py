# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Post-publish smoke test for the ``gmeow`` PyPI package.

Creates a fresh virtual environment, installs a pinned ``gmeow==<version>``
from PyPI (with bounded retry to absorb propagation delay), runs the agent-memory
example using a ``.gts`` file, and exercises both the ``gmeow`` and ``gts`` CLI
binaries.  The whole sequence is asserted to complete within five minutes
(Principle 13).
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

MAX_ELAPSED_SECONDS = 300
POST_INSTALL_BUDGET_SECONDS = 60
INSTALL_POLL_SECONDS = MAX_ELAPSED_SECONDS - POST_INSTALL_BUDGET_SECONDS
INSTALL_INTERVAL_SECONDS = 10


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
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    """Run a command with the inherited environment plus optional overrides."""
    return subprocess.run(
        args,
        check=check,
        env=env,
        timeout=timeout,
        capture_output=True,
        text=True,
    )


def _install_pinned(
    python: Path,
    package: str,
    version: str,
    timeout: int = INSTALL_POLL_SECONDS,
) -> None:
    """Install an exact version from PyPI, retrying until it propagates."""
    deadline = time.monotonic() + timeout
    specifier = f"{package}=={version}"
    attempt = 0
    while True:
        attempt += 1
        result = _run(
            [str(python), "-m", "pip", "install", "--upgrade", "pip", specifier],
            timeout=120,
            check=False,
        )
        if result.returncode == 0:
            print(f"installed {specifier} on attempt {attempt}")
            return
        if time.monotonic() >= deadline:
            print(
                f"FAIL: could not install {specifier} within {timeout}s "
                f"({attempt} attempts); last error:\n{result.stderr}",
                file=sys.stderr,
            )
            sys.exit(1)
        print(
            f"install attempt {attempt} failed; retrying in {INSTALL_INTERVAL_SECONDS}s"
        )
        time.sleep(INSTALL_INTERVAL_SECONDS)


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments and ``GMEOW_VERSION`` fallback."""
    parser = argparse.ArgumentParser(
        description="Post-publish smoke test for the gmeow PyPI package.",
    )
    parser.add_argument(
        "--version",
        default=os.environ.get("GMEOW_VERSION"),
        help="Exact gmeow version to install from PyPI (e.g. 1.0.2).",
    )
    args = parser.parse_args(argv)
    if not args.version:
        parser.error("--version or GMEOW_VERSION environment variable is required")
    return args


def main(argv: list[str] | None = None) -> int:
    """Install ``gmeow==<version>`` from PyPI and run the five-minute quickstart."""
    args = _parse_args(argv)
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

        # 2. Install the pinned published ``gmeow`` client from PyPI,
        #    reserving budget for the quickstart and CLI probes.
        remaining_install_budget = max(
            1,
            int(
                MAX_ELAPSED_SECONDS
                - (time.monotonic() - start)
                - POST_INSTALL_BUDGET_SECONDS
            ),
        )
        _install_pinned(
            python,
            "gmeow",
            args.version,
            timeout=remaining_install_budget,
        )

        # 3. Run the agent-memory example using a ``.gts`` file.
        assistant_literal = repr(str(assistant))
        quickstart = f"""
from gts.examples.agent_memory import Memory

mem = Memory({assistant_literal})
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
"""
        _run([str(python), "-c", quickstart])

        # 4. Verify the bundled ontology CLI and the engine CLI binaries.
        _run([str(bin_dir / "gmeow"), "--help"])
        _run([str(bin_dir / "gts"), "info", str(assistant)])

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
