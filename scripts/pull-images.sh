#!/usr/bin/env bash
# Pre-pull the pinned Docker images used by the GMEOW toolchain.
# Image references are read from gmeow_tools.config so they never drift.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

mapfile -t images < <(
	uv run python -c \
		"from gmeow_tools.config import ROBOT_IMAGE, WIDOCO_IMAGE, JENA_IMAGE; print(ROBOT_IMAGE); print(WIDOCO_IMAGE); print(JENA_IMAGE)"
)

for image in "${images[@]}"; do
	echo "Pulling ${image} ..."
	docker pull "${image}"
done

echo "✓ pinned images pulled"
