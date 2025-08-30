//! Extension traits and utilities for [tokio-stream](https://docs.rs/tokio-stream)'s `Stream` trait.
#![no_std]
#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms, single_use_lifetimes),
        allow(dead_code, unused_assignments, unused_variables)
    )
))]
#![warn(missing_docs)]

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod fused;
mod r#try;

pub use crate::fused::FusedStream;
pub use crate::r#try::TryStream;

#[cfg(feature = "sync")]
pub mod sync;
