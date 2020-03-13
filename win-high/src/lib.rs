//! # Various Rusty wrappers for low-level winapi
#[cfg(test)]
#[macro_use]
extern crate lazy_static;

pub mod format;
pub mod hkey;
pub mod error;
pub mod reg;
pub mod perf;
pub mod prelude;
