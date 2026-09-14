// src/core.rs
//! Domain Core: Business entities, state machines, and execution engines.

pub mod context;
pub mod executor;
pub mod projects;
pub mod protocol;
pub mod provider;
pub mod state;
pub mod task;
pub mod workspace;

pub use context::AppCore;