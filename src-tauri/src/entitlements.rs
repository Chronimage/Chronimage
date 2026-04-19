//! Entitlements — gate-ready scaffolding. Every potentially-paid feature goes
//! through [`ensure_entitlement`]. In v1 every check returns `Ok`; flipping a
//! later tier on is a one-file change here plus optional UI copy.

use crate::AppError;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Registry of every gate-able feature. **Add new features here** — don't
/// invent ad-hoc strings elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Feature {
    BulkSourceCleanup,
    CloudBackup,
    SemanticFaceSearch,
    ImportBatchLarge,
    PromptEdit,
    InsiderUpdates,
}

impl Feature {
    /// Stable string name for logging + telemetry.
    pub const fn name(self) -> &'static str {
        match self {
            Self::BulkSourceCleanup => "bulk-source-cleanup",
            Self::CloudBackup => "cloud-backup",
            Self::SemanticFaceSearch => "semantic-face-search",
            Self::ImportBatchLarge => "import-batch-large",
            Self::PromptEdit => "prompt-edit",
            Self::InsiderUpdates => "insider-updates",
        }
    }
}

/// Resolved entitlements for the current user/session. Loaded from
/// `license.json` (managed by `tauri-plugin-store`) at startup. In v1 every
/// field is `true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entitlements {
    pub plan: String,
    pub bulk_source_cleanup: bool,
    pub cloud_backup: bool,
    pub semantic_face_search: bool,
    pub import_batch_large: bool,
    pub prompt_edit: bool,
    pub insider_updates: bool,
}

impl Default for Entitlements {
    fn default() -> Self {
        Self {
            plan: "community".into(),
            bulk_source_cleanup: true,
            cloud_backup: true,
            semantic_face_search: true,
            import_batch_large: true,
            prompt_edit: true,
            insider_updates: false,
        }
    }
}

impl Entitlements {
    pub fn allows(&self, feature: Feature) -> bool {
        match feature {
            Feature::BulkSourceCleanup => self.bulk_source_cleanup,
            Feature::CloudBackup => self.cloud_backup,
            Feature::SemanticFaceSearch => self.semantic_face_search,
            Feature::ImportBatchLarge => self.import_batch_large,
            Feature::PromptEdit => self.prompt_edit,
            Feature::InsiderUpdates => self.insider_updates,
        }
    }
}

static CURRENT: OnceLock<Entitlements> = OnceLock::new();

/// Load the current entitlement set. In v1 returns the default (all-true except
/// insider). Phase 5+ will swap this to read a signed license.json.
pub fn current() -> &'static Entitlements {
    CURRENT.get_or_init(Entitlements::default)
}

/// Gate guard. Use at the top of any `#[tauri::command]` that might one day be
/// premium.
pub fn ensure(feature: Feature) -> Result<(), AppError> {
    if current().allows(feature) {
        Ok(())
    } else {
        Err(AppError::PermissionDenied(format!(
            "feature {} not enabled in current plan ({})",
            feature.name(),
            current().plan
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_allows_all_except_insider() {
        let e = Entitlements::default();
        assert!(e.allows(Feature::BulkSourceCleanup));
        assert!(e.allows(Feature::CloudBackup));
        assert!(e.allows(Feature::SemanticFaceSearch));
        assert!(e.allows(Feature::ImportBatchLarge));
        assert!(e.allows(Feature::PromptEdit));
        assert!(!e.allows(Feature::InsiderUpdates));
    }

    #[test]
    fn ensure_passes_when_enabled() {
        assert!(ensure(Feature::CloudBackup).is_ok());
    }

    #[test]
    fn feature_names_are_stable_kebab_case() {
        assert_eq!(Feature::BulkSourceCleanup.name(), "bulk-source-cleanup");
        assert_eq!(Feature::PromptEdit.name(), "prompt-edit");
    }
}
