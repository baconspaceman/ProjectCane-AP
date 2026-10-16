//! Portable client orchestration. Game adapters own game I/O and game-specific
//! persistence; existing clients can adopt individual modules without migrating
//! their save or seed contracts.

pub mod connection;
pub mod legacy_checks;
pub mod legacy_tick;
pub mod observer;
pub mod runner;
pub mod session;
pub mod store;
pub mod ui;

mod adapter;
mod host;

pub use adapter::*;
pub use host::*;
