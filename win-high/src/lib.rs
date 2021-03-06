//! # Various Rusty wrappers for low-level winapi
#[macro_use]
extern crate bitflags;
#[cfg(test)]
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate nom;

pub mod format;
pub mod hkey;
pub mod error;
pub mod reg;
pub mod perf;
pub mod prelude;
