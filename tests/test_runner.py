"""Tests for Docker runner timeout and leak hardening."""

from __future__ import annotations

import subprocess
from unittest.mock import MagicMock, patch

import pytest

from gmeow_tools.runner import (
    ToolExecutionError,
    ToolUnavailableError,
    docker_available,
    image_available,
    pull_image,
    run_container,
)


class FakeTimeoutExpired(subprocess.TimeoutExpired):
    """A TimeoutExpired with stdout/stderr attributes for compatibility."""

    def __init__(
        self,
        cmd: list[str],
        timeout: float,
        *,
        stdout: str | bytes | None = None,
        stderr: str | bytes | None = None,
    ) -> None:
        super().__init__(cmd, timeout)
        self.stdout = stdout  # type: ignore[assignment]
        self.stderr = stderr  # type: ignore[assignment]


def test_docker_available_returns_false_on_timeout() -> None:
    with patch(
        "gmeow_tools.runner.subprocess.run",
        side_effect=subprocess.TimeoutExpired(["docker", "info"], 15.0),
    ):
        assert docker_available() is False


def test_image_available_returns_false_on_timeout() -> None:
    with patch(
        "gmeow_tools.runner.subprocess.run",
        side_effect=subprocess.TimeoutExpired(
            ["docker", "image", "inspect", "x"], 15.0
        ),
    ):
        assert image_available("x") is False


def test_pull_image_raises_on_timeout() -> None:
    with (
        patch(
            "gmeow_tools.runner.subprocess.run",
            side_effect=subprocess.TimeoutExpired(["docker", "pull", "x"], 300.0),
        ),
        patch("gmeow_tools.runner.docker_available", return_value=True),
        pytest.raises(ToolUnavailableError, match="timed out"),
    ):
        pull_image("x")


def test_run_container_kills_and_raises_on_timeout() -> None:
    """On TimeoutExpired the container is killed and ToolExecutionError is raised."""
    docker_calls: list[list[str]] = []

    def fake_run(cmd: list[str], **kwargs: object) -> MagicMock:
        docker_calls.append(cmd)
        if cmd[1] == "run":
            raise FakeTimeoutExpired(cmd, timeout=1.0, stdout="out", stderr="err")
        # kill / rm succeed silently
        mock = MagicMock()
        mock.returncode = 0
        return mock

    with (
        patch("gmeow_tools.runner.shutil.which", return_value="/bin/docker"),
        patch("gmeow_tools.runner.subprocess.run", side_effect=fake_run),
        pytest.raises(ToolExecutionError, match="timed out"),
    ):
        run_container("stain/jena:5.4.0", ["riot", "--version"], timeout=1.0)

    # docker_available -> info, image_available -> image inspect, then run, kill, rm
    assert docker_calls[0][1] == "info"
    assert docker_calls[1][1:3] == ["image", "inspect"]
    assert docker_calls[2][1] == "run"
    assert docker_calls[3][1] == "kill"
    assert docker_calls[4][1:3] == ["rm", "-f"]
