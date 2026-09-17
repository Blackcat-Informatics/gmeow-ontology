// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers kept outside the selected production source closure.

use super::*;

impl ParsedAuthoredSources {
    /// Construct one explicitly supplied tiny source for a synthetic stage test.
    /// This does not discover, compile or generate a repository corpus.
    #[cfg(test)]
    pub(crate) fn synthetic(nquads: &str) -> Self {
        Self::synthetic_documents([("synthetic.nq", "application/n-quads", nquads)])
    }

    /// Explicit tiny documents with independent names and scopes; no filesystem reads.
    #[cfg(test)]
    pub(crate) fn synthetic_documents<'a>(
        documents: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    ) -> Self {
        Self {
            sources: documents
                .into_iter()
                .map(|(path, media_type, content)| ParsedAuthoredSource {
                    path: PathBuf::from(path),
                    relative_path: path.into(),
                    content_digest: purrdf::ContentDigest::of(content.as_bytes()).to_hex(),
                    blake3_digest: blake3::hash(content.as_bytes()).to_hex().to_string(),
                    kind: OriginKind::Source,
                    ingested: PurrdfAdapter
                        .ingest(path, media_type, content.as_bytes())
                        .expect("tiny synthetic source"),
                })
                .collect(),
        }
    }
}
