// src/core.rs
pub mod context;
pub mod executor;
pub mod projects;
pub mod protocol;
pub mod provider;
pub mod skill;
pub mod state;
pub mod task;
pub mod workspace;

pub use context::AppCore;
pub use skill::*;
