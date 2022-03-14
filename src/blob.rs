use byteorder::{BigEndian, ReadBytesExt};
use osm_pbf_proto::fileformat::mod_Blob::OneOfdata;
pub use osm_pbf_proto::fileformat::{Blob as PbfBlob, BlobHeader as PbfBlobHeader};
use osm_pbf_proto::quick_protobuf::{BytesReader, MessageRead};
use std::io::{self, Read};
use std::iter;
use std::marker::PhantomData;
use std::pin::Pin;
use std::str::FromStr;

use crate::data::OSMDataBlob;
use crate::error::{Error, Result};
use crate::header::OSMHeaderBlob;

const MAX_HEADER_SIZE: u32 = 64 * 1024;
const MAX_UNCOMPRESSED_DATA_SIZE: usize = 32 * 1024 * 1024;

pub struct Blob<M: 'static> {
    blob_type: String,
    data_bytes: Pin<Vec<u8>>,
    blob: PbfBlob<'static>,
    phantom: PhantomData<M>,
}

impl<M> Blob<M> {
    #[inline]
    pub fn blob_type(&self) -> &str {
        &self.blob_type
    }
}

impl<M> Blob<M> {
    #[inline]
    fn from_bytes(blob_type: String, data: Vec<u8>) -> Result<Self> {
        let data_bytes = Pin::new(data);
        // unlink lifetimes
        let bytes_self = unsafe { &*((&data_bytes as &[u8]) as *const [u8]) };
        let blob = PbfBlob::from_reader(&mut BytesReader::from_bytes(bytes_self), bytes_self)?;
        Ok(Blob {
            blob_type,
            data_bytes,
            blob,
            phantom: PhantomData,
        })
    }

    #[inline(always)]
    unsafe fn cast<N>(self) -> Blob<N> {
        Blob {
            blob_type: self.blob_type,
            data_bytes: self.data_bytes,
            blob: self.blob,
            phantom: PhantomData,
        }
    }
}

pub trait Block: Sized {
    fn read_from_bytes(bytes: &[u8]) -> Result<Self>;
}

impl<M: Block> Blob<M> {
    pub fn decode(self) -> Result<M> {
        match self.blob.data {
            OneOfdata::raw(bytes) => Ok(M::read_from_bytes(&bytes)?),
            OneOfdata::zlib_data(bytes) if cfg!(feature = "zlib") => {
                let raw_size = (self.blob.raw_size.unwrap_or_default() as usize).max(bytes.len());
                let cursor = io::Cursor::new(bytes);
                let mut decoder = flate2::bufread::ZlibDecoder::new(cursor);
                let mut bytes = Vec::with_capacity(raw_size);
                decoder.read_to_end(&mut bytes)?;
                Ok(M::read_from_bytes(&bytes)?)
            }
            OneOfdata::lzma_data(bytes) if cfg!(feature = "lzma") => {
                let raw_size = (self.blob.raw_size.unwrap_or_default() as usize).max(bytes.len());
                let cursor = io::Cursor::new(bytes);
                let mut decoder = xz2::bufread::XzDecoder::new(cursor);
                let mut bytes = Vec::with_capacity(raw_size);
                decoder.read_to_end(&mut bytes)?;
                Ok(M::read_from_bytes(&bytes)?)
            }
            _ => Err(Error::UnsupportedEncoding),
        }
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
            Some(blob) if blob.blob_type == "OSMHeader" => {
                // SAFETY: we have checked, that it is a OSMData blob
                Ok(unsafe { blob.cast() })
            }
            Some(blob) => Err(Error::UnexpectedBlobType(blob.blob_type)),
            None => Err(std::io::ErrorKind::UnexpectedEof.into()),
        }
    }

    fn read_bytes_exact(&mut self, exact_size: usize) -> Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(exact_size);
        let len = self
            .0
            .by_ref()
            .take(exact_size as u64)
            .read_to_end(&mut bytes)?;
        if len != exact_size {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(bytes)
    }

    fn next_blob(&mut self) -> Result<Option<Blob<()>>> {
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

        let header_bytes = self.read_bytes_exact(header_size)?;
        let header =
            PbfBlobHeader::from_reader(&mut BytesReader::from_bytes(&header_bytes), &header_bytes)?;
        let data_size = header.datasize as usize;
        if data_size > MAX_UNCOMPRESSED_DATA_SIZE {
            return Err(Error::BlobDataToLarge);
        }
        let blob_type = header.type_pb.into_owned();
        let blob_bytes = self.read_bytes_exact(data_size)?;
        let blob = Blob::from_bytes(blob_type, blob_bytes)?;
        Ok(Some(blob))
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
                Ok(Some(blob)) if blob.blob_type == "OSMData" => {
                    // SAFETY: we have checked, that it is a OSMData blob
                    return Some(Ok(unsafe { blob.cast() }));
                }
                // skip unsupported blobs and header-blobs
                _ => {}
            }
        }
    }
}

impl<R: io::BufRead> iter::FusedIterator for Blobs<R> {}
