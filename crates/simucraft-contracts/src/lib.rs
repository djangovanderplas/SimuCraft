//! Contracts between SimuCraft components (WP0.3). Changing anything public
//! here is a contract change: it gets its own pull request (see CLAUDE.md).
//!
//! - [`diagnostic`]: how every component reports problems
//! - [`block`]: what a block type declares and the methods a runtime calls
//! - [`ir`]: the compiled model the compiler writes and the runtime runs
//! - [`api`]: Model API commands (JSON-RPC 2.0)
//! - [`fake`]: small working stand-ins, so lanes can build before their
//!   neighbours exist

pub mod api;
pub mod block;
pub mod diagnostic;
pub mod fake;
pub mod ir;

pub use diagnostic::{Diagnostic, Fix, Severity, Span};
