//! SimuCraft project files (spec Sections 4 to 7).
//!
//! - [`check`] validates a file and returns diagnostics.
//! - [`File`] edits a file in place, keeping comments and layout.
//! - [`id`] makes and checks stable IDs.

mod check;
mod file;
pub mod id;
mod kind;
pub mod schema;

pub use check::{check, codes};
pub use file::File;
pub use kind::{FORMAT_VERSION, Kind};
