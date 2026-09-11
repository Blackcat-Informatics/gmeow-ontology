// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only admission of the exact corpus actions selected before tests start.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
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
    pub fn from_receipt<P: Serialize>(receipt: &ActionReceipt<P>) -> Self {
        Self {
            context: receipt.context.clone(),
            receipt_digest: receipt.digest(),
            product_digest: receipt.product_digest.clone(),
        }
    }

    /// Compare an authenticated store receipt with the producer's exact selection.
    pub fn verify<P: Serialize>(&self, receipt: &ActionReceipt<P>) -> Result<(), ActionCacheError> {
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
pub fn load_manifest<T: DeserializeOwned>(root: &Path) -> Result<T, ActionCacheError> {
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
pub fn read_manifest<T: DeserializeOwned>(
    path: &Path,
    expected: &str,
) -> Result<T, ActionCacheError> {
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
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(u64::from(MANIFEST_SCHEMA_VERSION))
    {
        return Err(ActionCacheError::message(
            "fixture selector schema mismatch",
        ));
    }
    Ok(serde_json::from_value(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_admission_requires_the_supplied_digest_and_current_schema() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("selector.json");
        let bytes = br#"{"schema_version":2,"fixture":"selected"}"#;
        std::fs::write(&path, bytes).unwrap();
        let digest = format!("{:x}", Sha256::digest(bytes));
        let admitted: serde_json::Value = read_manifest(&path, &digest).unwrap();
        assert_eq!(admitted["fixture"], "selected");
        assert!(read_manifest::<serde_json::Value>(&path, &"0".repeat(64)).is_err());
        assert!(read_manifest::<serde_json::Value>(&path, "").is_err());
        let stale = br#"{"schema_version":1,"fixture":"selected"}"#;
        std::fs::write(&path, stale).unwrap();
        let stale_digest = format!("{:x}", Sha256::digest(stale));
        assert!(read_manifest::<serde_json::Value>(&path, &stale_digest).is_err());
        std::fs::remove_file(path).unwrap();
        assert!(
            read_manifest::<serde_json::Value>(&directory.path().join("selector.json"), &digest)
                .is_err()
        );
    }
}
