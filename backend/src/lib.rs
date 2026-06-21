//! SubFixer: crowd-sourced closed-caption corrections.
//!
//! Library crate exposing the caption diff engine, the (video URL, timestamp)
//! key normalization, the SQLite-backed correction/fixer store, the leaderboard
//! ranking logic, and the Axum HTTP API. The `subfixer-server` binary is a thin
//! wrapper around [`api::router`].

pub mod api;
pub mod diff;
pub mod error;
pub mod leaderboard;
pub mod store;
pub mod videokey;

pub use error::SubFixerError;
pub use store::Store;
