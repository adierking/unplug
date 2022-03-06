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

pub mod atc;
pub mod item;
pub mod music;
pub mod object;
pub mod sfx_group;
pub mod sound;
pub mod sound_event;
pub mod stage;
pub mod suit;

pub use atc::Atc;
pub use item::Item;
pub use music::Music;
pub use object::Object;
pub use sfx_group::SfxGroup;
pub use sound::Sound;
pub use sound_event::SoundEvent;
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

    #[error("{0:?} does not have a corresponding suit")]
    NoItemSuit(Item),

    #[error("{0:?} does not have a corresponding item")]
    NoAtcItem(Atc),
}
