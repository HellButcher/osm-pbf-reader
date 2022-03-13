use std::string::FromUtf8Error;

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    DecodeError(#[from] osm_pbf_proto::prost::DecodeError),

    #[error(transparent)]
    Utf8Error(#[from] FromUtf8Error),

    // The length of the BlobHeader [..] must be less than 64 KiB.
    // https://wiki.openstreetmap.org/wiki/PBF_Format
    #[error("Invalid Format: The size of the `BlobHeader` is to large")]
    BlobHeaderToLarge,

    // The uncompressed length of a Blob [..] must be less than 32 MiB.
    // https://wiki.openstreetmap.org/wiki/PBF_Format
    #[error("Invalid Format: The size of the `Blob` is to large")]
    BlobDataToLarge,
    
    #[error("The encoding of the Blob is not supported")]
    UnsupportedEncoding,

    #[error("Unexpected Blob-Type {0}")]
    UnexpectedBlobType(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<std::io::ErrorKind> for Error {
    #[inline(always)]
    fn from(kind: std::io::ErrorKind) -> Self {
        Error::IoError(kind.into())
    }
}
