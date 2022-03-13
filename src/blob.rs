use byteorder::{BigEndian, ReadBytesExt};
use osm_pbf_proto::fileformat::blob::Data;
pub use osm_pbf_proto::fileformat::{Blob as PbfBlob, BlobHeader as PbfBlobHeader};
use osm_pbf_proto::prost::Message;
use osm_pbf_proto::prost::bytes::Buf;
use std::io::{self, Read};
use std::iter;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;

use crate::data::OSMDataBlob;
use crate::error::{Error, Result};
use crate::header::OSMHeaderBlob;

const MAX_HEADER_SIZE: u32 = 64 * 1024;
const MAX_UNCOMPRESSED_DATA_SIZE: usize = 32 * 1024 * 1024;

pub struct Blob<M> {
    header: PbfBlobHeader,
    blob: PbfBlob,
    phantom: PhantomData<M>,
}

impl<M> Blob<M> {
    #[inline]
    const fn new(header: PbfBlobHeader, blob: PbfBlob) -> Self {
        Blob {
            header,
            blob,
            phantom: PhantomData,
        }
    }
}

pub trait Block: Sized {
    type Message: Message + Default;

    fn from_message(pbf: Self::Message) -> Result<Self>;

    fn decode(buf: impl Buf) -> Result<Self> {
        let msg = Self::Message::decode(buf)?;
        let block = Self::from_message(msg)?;
        Ok(block)
    }
}

impl<M: Block> Blob<M> {
    pub fn decode(&self) -> Result<M> {
        match &self.blob.data {
            Some(Data::Raw(bytes)) => Ok(M::decode(bytes.as_slice())?),
            Some(Data::ZlibData(bytes)) if cfg!(feature = "zlib") => {
                let raw_size = (self.blob.raw_size.unwrap_or_default() as usize).max(bytes.len());
                let cursor = io::Cursor::new(bytes);
                let mut decoder = flate2::bufread::ZlibDecoder::new(cursor);
                let mut bytes = Vec::with_capacity(raw_size);
                decoder.read_to_end(&mut bytes)?;
                Ok(M::decode(bytes.as_slice())?)
            },
            Some(Data::LzmaData(bytes)) if cfg!(feature = "lzma") => {
                let raw_size = (self.blob.raw_size.unwrap_or_default() as usize).max(bytes.len());
                let cursor = io::Cursor::new(bytes);
                let mut decoder = xz2::bufread::XzDecoder::new(cursor);
                let mut bytes = Vec::with_capacity(raw_size);
                decoder.read_to_end(&mut bytes)?;
                Ok(M::decode(bytes.as_slice())?)
            },
            _ => Err(Error::UnsupportedEncoding),
        }
    }
}

impl<M> Deref for Blob<M> {
    type Target = PbfBlobHeader;
    #[inline]
    fn deref(&self) -> &PbfBlobHeader {
        &self.header
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BlobType {
    OSMHeader,
    OSMData,
}

impl FromStr for BlobType {
    type Err = ();
    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "OSMHeader" => Self::OSMHeader,
            "OSMData" => Self::OSMData,
            _ => return Err(()),
        })
    }
}

#[derive(Debug)]
pub struct Blobs<R>(R);

impl<R> Blobs<R> {
    #[inline]
    pub fn into_inner(self) -> R {
        self.0
    }
}

impl<R: AsRef<[u8]>> Blobs<io::Cursor<R>> {
    #[inline]
    pub fn from_bytes(bytes: R) -> Self {
        Self(io::Cursor::new(bytes))
    }
}

impl<R: io::Read> Blobs<io::BufReader<R>> {
    #[inline]
    pub fn from_read(read: R) -> Self {
        Self(io::BufReader::new(read))
    }
}

impl<R: io::Seek> Blobs<R> {
    #[inline]
    pub fn rewind(&mut self) -> io::Result<()> {
        self.0.rewind()?;
        Ok(())
    }
}

impl<R: io::BufRead> Blobs<R> {
    #[inline]
    pub fn from_buf_read(read: R) -> Self {
        Self(read)
    }

    pub fn header(&mut self) -> Result<OSMHeaderBlob> {
        match self.next_blob()? {
            Some((header, blob)) if header.r#type == "OSMHeader" => {
                Ok(OSMHeaderBlob::new(header, blob))
            }
            Some((header, _)) => Err(Error::UnexpectedBlobType(header.r#type)),
            None => Err(std::io::ErrorKind::UnexpectedEof.into()),
        }
    }

    fn read_msg_exact<M: Message + Default>(&mut self, exact_size: usize) -> Result<M> {
        let mut bytes = Vec::with_capacity(exact_size);
        let len = self.0.by_ref().take(exact_size as u64).read_to_end(&mut bytes)?;
        if len != exact_size {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let msg = M::decode(bytes.as_slice())?;
        Ok(msg)
    }

    fn next_blob(&mut self) -> Result<Option<(PbfBlobHeader, PbfBlob)>> {
        let header_size = match self.0.read_u32::<BigEndian>() {
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(None); // Expected EOF
            }
            Err(e) => return Err(Error::IoError(e)),
            Ok(header_size) if header_size > MAX_HEADER_SIZE => {
                return Err(Error::BlobHeaderToLarge);
            }
            Ok(header_size) => header_size as usize,
        };
        
        let header: PbfBlobHeader = self.read_msg_exact(header_size)?;
        let data_size = header.datasize as usize;
        if data_size > MAX_UNCOMPRESSED_DATA_SIZE {
            return Err(Error::BlobDataToLarge);
        }

        let blob: PbfBlob = self.read_msg_exact(data_size)?;
        Ok(Some((header, blob)))
    }
}

impl<R: io::BufRead> iter::Iterator for Blobs<R> {
    type Item = Result<OSMDataBlob>;

    fn next(&mut self) -> Option<Result<OSMDataBlob>> {
        loop {
            match self.next_blob() {
                Err(e) => {
                    return Some(Err(e));
                }
                Ok(None) => {
                    return None;
                }
                Ok(Some((header, blob))) if header.r#type == "OSMData" => {
                    return Some(Ok(OSMDataBlob::new(header, blob)));
                }
                // skip unsupported blobs and header-blobs
                _ => {}
            }
        }
    }
}

impl<R: io::BufRead> iter::FusedIterator for Blobs<R> {}
