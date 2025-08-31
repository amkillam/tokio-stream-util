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

pub mod fused;
pub mod try_stream;

pub use crate::fused::FusedStream;
pub use crate::try_stream::{TryStream, TryStreamExt};

#[cfg(feature = "alloc")]
pub mod futures_ordered;
#[cfg(feature = "alloc")]
pub use futures_ordered::FuturesOrdered;

#[cfg(feature = "alloc")]
pub mod futures_unordered;

#[cfg(feature = "alloc")]
pub use futures_unordered::FuturesUnordered;

#[cfg(feature = "alloc")]
pub mod flatten_unordered;

#[cfg(feature = "alloc")]
pub use flatten_unordered::FlattenUnordered;

#[cfg(feature = "sync")]
pub mod sync;
