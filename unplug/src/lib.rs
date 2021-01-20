#![deny(
    absolute_paths_not_starting_with_crate,
    elided_lifetimes_in_paths,
    explicit_outlives_requirements,
    single_use_lifetimes,
    trivial_casts,
    trivial_numeric_casts,
    unconditional_recursion,
    unreachable_patterns,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes,
    unused_must_use,
    unused_qualifications,
    variant_size_differences
)]

pub use unplug_data as data;

#[macro_use]
mod macros;

pub mod common;
pub mod dvd;
pub mod event;
pub mod globals;
pub mod stage;

#[cfg(test)]
#[macro_use]
mod test;
