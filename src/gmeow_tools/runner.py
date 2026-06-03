"""Subprocess helpers for the pinned Docker toolchain (ROBOT, WIDOCO, Jena).

The Java tools run as pinned Docker images rather than local installs. Every
container mounts the repository at ``/work`` and runs as the invoking user
(``--user uid:gid``) so generated artifacts are never owned by root.

Two failure modes are distinguished:

* A *required* tool that is missing raises :class:`ToolUnavailableError`
  (fail-fast — ETHOS: no silent degradation).
* A *gated* tool (one the plan allows to skip, e.g. Jena for the preview RDF 1.2
  view) is checked with :func:`image_available` so the caller can skip with a
  visible warning.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from collections.abc import Sequence
from pathlib import Path

from gmeow_tools.config import PROJECT_ROOT


class ToolUnavailableError(RuntimeError):
    """Raised when a required external tool (Docker or an image) is missing."""


class ToolExecutionError(RuntimeError):
    """Raised when an external tool runs but exits non-zero."""

    def __init__(self, command: Sequence[str], returncode: int, output: str) -> None:
        """Store the failed command, its exit code, and combined output."""
        self.command = list(command)
        self.returncode = returncode
        self.output = output
        super().__init__(
            f"command exited {returncode}: {' '.join(self.command)}\n{output}"
        )


def docker_available() -> bool:
    """Return whether the Docker CLI is on PATH and the daemon responds."""
    if shutil.which("docker") is None:
        return False
    result = subprocess.run(
        ["docker", "info"],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode == 0


def image_available(image: str) -> bool:
    """Return whether a Docker image is present locally (does not pull)."""
    if shutil.which("docker") is None:
        return False
    result = subprocess.run(
        ["docker", "image", "inspect", image],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode == 0


def pull_image(image: str) -> None:
    """Pull a Docker image, raising :class:`ToolUnavailableError` on failure."""
    if not docker_available():
        raise ToolUnavailableError("Docker is not available")
    result = subprocess.run(
        ["docker", "pull", image],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise ToolUnavailableError(f"could not pull {image}: {result.stderr.strip()}")


def _user_spec() -> str:
    """Return the ``uid:gid`` of the current process for ``docker --user``."""
    return f"{os.getuid()}:{os.getgid()}"


def run_container(
    image: str,
    args: Sequence[str],
    *,
    workdir: Path = PROJECT_ROOT,
    network: bool = False,
    check: bool = True,
    timeout: float | None = 900,
) -> subprocess.CompletedProcess[str]:
    """Run a command in a pinned Docker image with the repo mounted at /work.

    Args:
        image: The pinned image reference (see ``config``).
        args: The command and arguments to run inside the container.
        workdir: Host directory to mount at ``/work`` (defaults to the repo root).
        network: If ``False`` (default), run with ``--network none`` for
            hermetic, deterministic builds. Set ``True`` only for steps that
            must reach the network.
        check: Raise :class:`ToolExecutionError` on a non-zero exit.
        timeout: Seconds before the container is killed.

    Returns:
        The completed process (stdout/stderr captured as text).

    Raises:
        ToolUnavailableError: If Docker or the image is unavailable.
        ToolExecutionError: If ``check`` is set and the command exits non-zero.
    """
    if not docker_available():
        raise ToolUnavailableError("Docker is not available")
    if not image_available(image):
        raise ToolUnavailableError(f"Docker image not present locally: {image}")

    docker_cmd = [
        "docker",
        "run",
        "--rm",
        "--user",
        _user_spec(),
        "--volume",
        f"{workdir}:/work",
        "--workdir",
        "/work",
    ]
    if not network:
        docker_cmd += ["--network", "none"]
    docker_cmd.append(image)
    docker_cmd += list(args)

    result = subprocess.run(
        docker_cmd,
        capture_output=True,
        text=True,
        check=False,
        timeout=timeout,
    )
    if check and result.returncode != 0:
        raise ToolExecutionError(
            docker_cmd, result.returncode, result.stdout + result.stderr
        )
    return result
