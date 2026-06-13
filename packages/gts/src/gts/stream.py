# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The spec-owned ``stream`` vocabulary (GTS-SPEC §13.3).

Constants only — the streaming-index terms a streamable segment leads with
(§3.3) and the compaction-provenance terms a streamable rewrite records
(§10.1). Like the ``files`` profile vocabulary (§13.2), the terms are authored
in the spec and carried as literal IRIs; no external ontology is required.
"""

from __future__ import annotations

STREAM_NS = "https://w3id.org/gts/stream#"

# Streaming-index terms (§3.3): one Manifestation per promised blob.
MANIFESTATION = STREAM_NS + "Manifestation"
DIGEST = STREAM_NS + "digest"
MEDIA_TYPE = STREAM_NS + "mediaType"
SIZE = STREAM_NS + "size"
ROLE = STREAM_NS + "role"
ORDER = STREAM_NS + "order"

# Compaction-provenance terms (§10.1).
COMPACTION = STREAM_NS + "Compaction"
AGENT = STREAM_NS + "agent"
TIMESTAMP = STREAM_NS + "timestamp"
SOURCE_HEAD = STREAM_NS + "sourceHead"
SEALED_SOURCE = STREAM_NS + "sealedSource"
DETACHED_SIGNATURE = STREAM_NS + "DetachedSignature"
SOURCE_FRAME = STREAM_NS + "sourceFrame"
COSE = STREAM_NS + "cose"

#: The fixed compactor identity recorded as ``stream:agent`` — a constant so
#: the rewrite is byte-reproducible across engines (§14.1 determinism).
COMPACT_AGENT = "gts-compact"
