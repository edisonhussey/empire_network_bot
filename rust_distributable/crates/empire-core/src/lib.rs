//! Small, UI-independent primitives shared by the daemon and desktop app.

#[cfg(feature = "application")]
pub mod account;
#[cfg(feature = "application")]
pub mod event;
pub mod hunt;
#[cfg(feature = "application")]
pub mod injection;
#[cfg(feature = "application")]
pub mod licence;
pub mod pacing;
pub mod paths;
pub mod protocol;
#[cfg(feature = "application")]
pub mod scheduler;
pub mod session;
#[cfg(feature = "application")]
pub mod store;

#[cfg(feature = "application")]
pub const RECENT_MESSAGE_LIMIT: i64 = 200;
#[cfg(feature = "application")]
pub const INJECTION_QUEUE_CAPACITY: usize = 64;
