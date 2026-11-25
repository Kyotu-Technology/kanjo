//! Kanjo core: domain types, storage, pricing and the API contract.
//!
//! This crate has no HTTP, no file watching and no OS integration. Everything
//! else in the workspace builds on the types and the `Store` defined here.

pub mod api;
pub mod config;
pub mod error;
pub mod limits;
pub mod model;
pub mod paths;
pub mod pricing;
pub mod settings;
pub mod spool;
pub mod statusline;
pub mod store;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
pub mod time;

pub use error::{Error, Result};
pub use store::Store;
pub use time::Ctx;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
