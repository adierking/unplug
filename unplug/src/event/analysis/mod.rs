mod analyzer;
mod block;
mod state;
mod subroutine;
mod value;

pub use analyzer::*;
pub use block::*;
pub use subroutine::*;
pub use value::*;

use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid library call: {0}")]
    InvalidLibrary(i16),
}

pub type Result<T> = std::result::Result<T, Error>;
