//! The on-disk state schema (serde).
//!
//! The JSON fields are `last_applied`, `applied_at`, `links`, `target`, and
//! `source`. Two compatibility behaviors are worth noting:
//!
//! - **`null` slices:** serde would reject a JSON `null` for a `Vec`, so
//!   `last_applied`/`links` use a deserializer that maps both a missing field
//!   and an explicit `null` to an empty vec.
//! - **zero time:** a missing `applied_at` defaults to the
//!   `0001-01-01T00:00:00Z` sentinel rather than erroring.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize};

/// The on-disk state representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// Profile names from the last `apply`, in order. Empty when applied via
    /// an explicit `-c/--config` path.
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub last_applied: Vec<String>,
    /// When the last apply ran (UTC). Defaults to the year-0001 sentinel.
    #[serde(default = "epoch_sentinel")]
    pub applied_at: DateTime<Utc>,
    /// Symlinks the engine created or confirmed.
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub links: Vec<Link>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            last_applied: Vec::new(),
            applied_at: epoch_sentinel(),
            links: Vec::new(),
        }
    }
}

/// One symlink the engine created or confirmed. Paths are stored as provided
/// (post tilde expansion) so `doctor` can re-stat them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// The symlink path.
    pub target: String,
    /// What it points at.
    pub source: String,
}

/// The zero-time sentinel: `0001-01-01T00:00:00Z`.
pub(crate) fn epoch_sentinel() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0)
        .single()
        .expect("valid sentinel date")
}

/// Deserialize a JSON `null` (or a missing field) as an empty vec.
fn null_as_empty_vec<'de, D, T>(de: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(de)?.unwrap_or_default())
}
