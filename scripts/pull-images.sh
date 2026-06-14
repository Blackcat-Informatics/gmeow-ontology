#!/usr/bin/env bash
# Pre-pull (or build) the pinned Docker images used by the GMEOW toolchain.
# Image references are read from gmeow_tools.config so they never drift.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

read -r ROBOT_IMAGE JENA_IMAGE < <(
	uv run python -c \
		"from gmeow_tools.config import ROBOT_IMAGE, JENA_IMAGE; print(ROBOT_IMAGE, JENA_IMAGE)"
)

echo "Pulling ${ROBOT_IMAGE} ..."
docker pull "${ROBOT_IMAGE}"

# Apache Jena: no maintained public 5.4 CLI image exists, so pull if available
# (e.g. a private mirror) and otherwise build the pinned tag from docker/jena/.
echo "Obtaining ${JENA_IMAGE} (RDF 1.2 engine) ..."
if ! docker pull "${JENA_IMAGE}" 2>/dev/null; then
	echo "  pull unavailable — building from docker/jena/Dockerfile"
	docker build -t "${JENA_IMAGE}" docker/jena
fi

echo "✓ pinned images ready"
