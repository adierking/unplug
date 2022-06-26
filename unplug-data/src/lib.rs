#![warn(
    absolute_paths_not_starting_with_crate,
    elided_lifetimes_in_paths,
    explicit_outlives_requirements,
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

mod atc;
mod item;
mod music;
mod object;
mod resource;
mod sfx;
mod sfx_group;
mod sfx_sample;
mod sound;
mod stage;
mod suit;

pub use atc::Atc;
pub use item::{Item, ItemFlags};
pub use music::Music;
pub use object::Object;
pub use resource::{Resource, ResourceIterator};
pub use sfx::Sfx;
pub use sfx_group::SfxGroup;
pub use sfx_sample::SfxSample;
pub use sound::Sound;
pub use stage::Stage;
pub use suit::Suit;

use thiserror::Error;

/// The result type for data operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for data operations.
#[derive(Error, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    #[error("{0:?} does not have a corresponding ATC")]
    NoItemAtc(Item),

    #[error("{0:?} does not have a corresponding item")]
    NoAtcItem(Atc),

    #[error("{0:?} does not have a corresponding object")]
    NoItemObject(Item),

    #[error("{0:?} does not have a corresponding item")]
    NoObjectItem(Object),

    #[error("{0:?} does not have a corresponding suit")]
    NoItemSuit(Item),

    #[error("{0:?} does not have a corresponding item")]
    NoSuitItem(Suit),

    #[error("invalid sound ID: 0x{0:>08x}")]
    InvalidSoundId(u32),
}

mod private {
    pub trait Sealed {}
}
