//! Contracts between SimuCraft components. Changing anything public here is a
//! contract change: it needs its own pull request (see CLAUDE.md).

pub mod diagnostic;

pub use diagnostic::{Diagnostic, Fix, Severity, Span};
