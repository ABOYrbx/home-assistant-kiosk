//! Failure tracking for an installed extension.
//!
//! A failing extension keeps its last good data. Repeated failures mark it
//! degraded so the shop and the status page can say so, and the app never
//! crashes because a third-party script broke.

use serde::{Deserialize, Serialize};

use super::host::{ExtensionError, ExtensionHost};
use super::output::ExtensionOutput;

pub(crate) const DEGRADED_AFTER_CONSECUTIVE_FAILURES: u32 = 3;

/// The visible health of one extension.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionStatus {
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
    pub degraded: bool,
    pub last_success: Option<String>,
}

/// Runs one extension and tracks its last good output.
pub struct ExtensionRunner {
    host: ExtensionHost,
    last_good: Option<ExtensionOutput>,
    status: ExtensionStatus,
    degraded_after: u32,
}

impl ExtensionRunner {
    pub fn new(host: ExtensionHost) -> Self {
        Self {
            host,
            last_good: None,
            status: ExtensionStatus::default(),
            degraded_after: DEGRADED_AFTER_CONSECUTIVE_FAILURES,
        }
    }

    /// Refresh the extension and update its health.
    ///
    /// On success the new output replaces the last good one and the failure
    /// counter resets. On failure the last good output is kept.
    pub fn refresh(&mut self) -> Result<ExtensionOutput, ExtensionError> {
        match self.host.refresh() {
            Ok(output) => {
                self.last_good = Some(output.clone());
                self.status.consecutive_failures = 0;
                self.status.degraded = false;
                self.status.last_error = None;
                self.status.last_success = Some(self.host.now_rfc3339());
                Ok(output)
            }
            Err(error) => {
                self.status.consecutive_failures =
                    self.status.consecutive_failures.saturating_add(1);
                self.status.degraded =
                    self.status.consecutive_failures >= self.degraded_after.max(1);
                self.status.last_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    pub fn host(&self) -> &ExtensionHost {
        &self.host
    }

    pub fn last_good(&self) -> Option<&ExtensionOutput> {
        self.last_good.as_ref()
    }

    pub fn status(&self) -> &ExtensionStatus {
        &self.status
    }
}
