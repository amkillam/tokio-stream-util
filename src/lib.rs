//! A convenience for streams that return `Result` values that includes
//! a variety of adapters tailored to such futures.
#![no_std]
#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms, single_use_lifetimes),
        allow(dead_code, unused_assignments, unused_variables)
    )
))]
#![warn(missing_docs, /* unsafe_op_in_unsafe_fn */)] // unsafe_op_in_unsafe_fn requires Rust 1.52

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;


mod r#try;
mod fused;

pub use crate::r#try::TryStream;
pub use crate::fused::FusedStream;
