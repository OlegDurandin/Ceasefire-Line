use serde::{Deserialize, Serialize};

/// A playing side (e.g. Israel vs. Egypt+Syria). Scenario defines the meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SideId(pub u8);
