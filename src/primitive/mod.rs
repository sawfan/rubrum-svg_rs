//! SVG authoring primitives.
//!
//! This module is intentionally backend-agnostic and contains:
//! - XML escaping helpers
//! - Attribute parsing/patching helpers for SVG tags
//! - Small DOM constructors for common SVG elements
//! - Convenience helpers for pointer hit-target elements

mod dom;
mod emit;
mod text;

pub use dom::*;
pub use emit::*;
pub use text::*;
