// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native HTTP transport for SPARQL `SERVICE` federation (S6b #928).
//!
//! [`HttpRemoteQuerySource`] is the production [`RemoteQuerySource`]: it POSTs the
//! forwarded query to a remote SPARQL endpoint over HTTP (via `ureq`, rustls) and
//! decodes the `application/sparql-results+json` response with the wasm-clean
//! [`gmeow_sparql_results::from_json`] reader.
//!
//! # wasm boundary
//!
//! This module is compiled **only off `wasm32`** (`ureq` is not wasm-portable).
//! The `ureq` dependency is itself gated to non-wasm targets in `Cargo.toml`, so
//! the wasm32 build of `gmeow-sparql-eval` never sees an HTTP client and the
//! wasm-first gate (`make rdf-core-hygiene`) stays green. The wasm query path can
//! still evaluate `SERVICE` through any other [`RemoteQuerySource`] (e.g. an
//! in-memory or host-provided source).

use std::io::Read as _;
use std::time::Duration;

use gmeow_sparql_algebra::Variable;

use crate::remote::{RemoteError, RemoteQuerySource, ResolvedBindings};

/// The default per-request timeout for a federated `SERVICE` call.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A [`RemoteQuerySource`] that forwards queries to a remote SPARQL endpoint over
/// HTTP. Reusable across endpoints (the endpoint URL is per-call).
#[derive(Debug, Clone)]
pub struct HttpRemoteQuerySource {
    timeout: Duration,
    user_agent: String,
}

impl Default for HttpRemoteQuerySource {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            user_agent: "gmeow-sparql-eval/0.1 (SERVICE federation)".to_owned(),
        }
    }
}

impl HttpRemoteQuerySource {
    /// A source with the default 30s timeout.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the per-request timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl RemoteQuerySource for HttpRemoteQuerySource {
    fn query(&self, endpoint: &str, query_text: &str) -> Result<ResolvedBindings, RemoteError> {
        // POST the query as `application/sparql-query`, asking for SPARQL Results
        // JSON (SPARQL 1.1 Protocol §2.1.3).
        let response = ureq::post(endpoint)
            .header("User-Agent", &self.user_agent)
            .header("Content-Type", "application/sparql-query")
            .header("Accept", "application/sparql-results+json")
            .config()
            .timeout_global(Some(self.timeout))
            .build()
            .send(query_text)
            .map_err(|e| RemoteError::Transport(format!("POST <{endpoint}>: {e}")))?;

        let mut body = Vec::new();
        response
            .into_body()
            .into_reader()
            .read_to_end(&mut body)
            .map_err(|e| RemoteError::Transport(format!("reading <{endpoint}> response: {e}")))?;

        let parsed = gmeow_sparql_results::from_json(&body).map_err(|e| {
            RemoteError::Decode(format!("SPARQL-results JSON from <{endpoint}>: {e}"))
        })?;

        Ok(ResolvedBindings {
            variables: parsed.variables.into_iter().map(Variable::new).collect(),
            rows: parsed.rows,
        })
    }
}
