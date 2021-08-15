#![warn(
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

pub mod commands;
pub mod common;
pub mod globals;
pub mod io;
pub mod msg;
pub mod opt;
pub mod shop;
