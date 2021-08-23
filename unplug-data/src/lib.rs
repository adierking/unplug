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

pub mod atc;
pub mod item;
pub mod object;
pub mod stage;
pub mod suit;

use atc::AtcId;
use item::ItemId;
use thiserror::Error;

/// The result type for data operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for data operations.
#[derive(Error, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    #[error("{0:?} does not have a corresponding ATC")]
    NoItemAtc(ItemId),

    #[error("{0:?} does not have a corresponding suit")]
    NoItemSuit(ItemId),

    #[error("{0:?} does not have a corresponding item")]
    NoAtcItem(AtcId),
}
