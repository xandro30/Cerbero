//! The CA of Cerbero: its aggregate, the format of the copy of the root CA key and the lifetimes
//! that are fixed (research R15).

use time::Duration;

pub mod authority;
pub mod backup;

pub const ROOT_LIFETIME: Duration = Duration::days(3650);
pub const INTERMEDIATE_LIFETIME: Duration = Duration::days(365);
/// How much earlier than the issuance a certificate of a component or of Hécate starts, to absorb
/// small clock differences.
pub const NOT_BEFORE_MARGIN: Duration = Duration::minutes(1);
