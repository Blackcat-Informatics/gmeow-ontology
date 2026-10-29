// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only admission of the exact corpus actions selected before tests start.

pub mod source_artifacts;

use std::io::Read as _;
use std::path::{Path, PathBuf};

use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{ActionCacheError, ActionContext, ActionReceipt};

pub const MANIFEST_PATH_ENV: &str = "GMEOW_TEST_FIXTURE_MANIFEST";
pub const MANIFEST_SHA256_ENV: &str = "GMEOW_TEST_FIXTURE_MANIFEST_SHA256";
pub const MANIFEST_SCHEMA_VERSION: u32 = 2;
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// An action selected by the producer, independent of the consumer's build profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedAction {
    pub context: ActionContext,
    pub receipt_digest: String,
    pub product_digest: String,
}

impl SelectedAction {
    #[must_use]
    pub fn from_receipt<P: ToJson>(receipt: &ActionReceipt<P>) -> Self {
        Self {
            context: receipt.context.clone(),
            receipt_digest: receipt.digest(),
            product_digest: receipt.product_digest.clone(),
        }
    }

    /// Compare an authenticated store receipt with the producer's exact selection.
    pub fn verify<P: ToJson>(&self, receipt: &ActionReceipt<P>) -> Result<(), ActionCacheError> {
        if self.context != receipt.context
            || self.context.key() != receipt.action_key
            || self.receipt_digest != receipt.digest()
            || self.product_digest != receipt.product_digest
        {
            return Err(ActionCacheError::message(
                "action receipt differs from the producer-selected fixture",
            ));
        }
        Ok(())
    }
}

/// Load only the manifest whose path and SHA-256 the runner supplied. No discovery,
/// producer, cache mutation, or checkout-derived action key is reachable here.
pub fn load_manifest<T: FromJson>(root: &Path) -> Result<T, ActionCacheError> {
    let path = std::env::var_os(MANIFEST_PATH_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| ActionCacheError::message(format!("{MANIFEST_PATH_ENV} is required")))?;
    let expected = std::env::var(MANIFEST_SHA256_ENV).map_err(|_| {
        ActionCacheError::message(format!(
            "{MANIFEST_SHA256_ENV} must select one exact SHA-256"
        ))
    })?;
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    read_manifest(&path, &expected)
}

/// Authenticate a bounded selector before decoding its caller-owned fields.
pub fn read_manifest<T: FromJson>(path: &Path, expected: &str) -> Result<T, ActionCacheError> {
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ActionCacheError::message(
            "fixture selector requires an exact SHA-256",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(ActionCacheError::message(
            "fixture selector exceeds its byte limit",
        ));
    }
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(ActionCacheError::message(format!(
            "fixture selector identity mismatch: expected {expected}, actual {actual}"
        )));
    }
    let value = purrdf_lex::json::read_slice(&bytes, purrdf_lex::json::Limits::DEFAULT)
        .map_err(|error| ActionCacheError::message(format!("action cache JSON: {error}")))?;
    let mut header = Record::new(&value, "fixture selector")
        .map_err(|error| ActionCacheError::message(format!("action cache JSON: {error}")))?;
    let schema = header
        .raw("schema_version")
        .map_err(|error| ActionCacheError::message(format!("action cache JSON: {error}")))?;
    if schema.and_then(Value::as_u64) != Some(u64::from(MANIFEST_SCHEMA_VERSION)) {
        return Err(ActionCacheError::message(
            "fixture selector schema mismatch",
        ));
    }
    T::from_json(&value)
        .map_err(|error| ActionCacheError::message(format!("action cache JSON: {error}")))
}

#[path = "selection.tests.rs"]
#[cfg(test)]
mod tests;

::purrdf_lex::json_record! { impl ToJson for SelectedAction {
    "context" => context,
    "receipt_digest" => receipt_digest,
    "product_digest" => product_digest,
}}

impl FromJson for SelectedAction {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SelectedAction")?;
        let result = Self {
            context: fields.required("context")?,
            receipt_digest: fields.required("receipt_digest")?,
            product_digest: fields.required("product_digest")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
