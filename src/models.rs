// src/models.rs
//! Root Models Facade.
//!
//! Strictly follows Rule 4: acts as a clean facade re-exporting all submodules
//! to guarantee zero breaking changes to external callers.

pub mod ai;
pub mod console;
pub mod core;

pub use ai::*;
pub use console::*;
pub use core::*;