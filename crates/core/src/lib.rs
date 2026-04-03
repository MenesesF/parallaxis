//! # Parallaxis Core
//!
//! Fundamental types shared across all Parallaxis crates.
//!
//! "Pode não ser tão fluente. Pode não conversar tão bonito.
//! Mas quando disser algo, você pode confiar — ou pelo menos
//! auditar por que disse."

pub mod types;
pub mod claim;
pub mod verification;
pub mod error;

pub use types::*;
pub use claim::*;
pub use verification::*;
pub use error::*;
