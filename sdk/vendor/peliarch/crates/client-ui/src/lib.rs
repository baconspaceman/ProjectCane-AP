//! Compatibility facade for the universal client's renderer-independent state.
//!
//! Existing game workers and renderers retain the `client_ui` paths. The definitions now live in
//! `peliarch_client_core::ui`, so adapters and legacy clients exchange exactly the same types and
//! serialized snapshots without conversion or a persistence migration.

pub use peliarch_client_core::ui::*;
