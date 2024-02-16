use byteorder::{BigEndian, ReadBytesExt};
use crossbeam_queue::ArrayQueue;
use osm_pbf_proto::quick_protobuf::BytesReader;
pub use osm_pbf_proto::{
    fileformat::{
        mod_Blob::OneOfdata as PbfBlobData, Blob as PbfBlob, BlobHeader as PbfBlobHeader,
    },
    quick_protobuf::{self as qpb, MessageRead},
};
use std::{fmt, marker::PhantomData};
use std::{
    io::{self, Read},
    sync::Arc,
};
use std::{iter, pin::Pin};

use crate::data::OSMDataBlob;
use crate::error::{Error, Result};
use crate::header::OSMHeaderBlob;

const MAX_HEADER_SIZE: u32 = 64 * 1024;
const MAX_UNCOMPRESSED_DATA_SIZE: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlobType {
    OSMHeader,
    OSMData,
    Other(String),
}

impl BlobType {
    fn from_str(s: &str) -> Self {
        match s {
            "OSMHeader" => Self::OSMHeader,
            "OSMData" => Self::OSMData,
            other => Self::Other(other.to_string()),
        }
    }
    fn as_str(&self) -> &str {
        match self {
            Self::OSMHeader => "OSMHeader",
            Self::OSMData => "OSMData",
            Self::Other(ref s) => s,
        }
    }
}

impl fmt::Display for BlobType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

struct BlobData {
    blob: PbfBlob<'static>,
    data: Pin<Vec<u8>>,
    pool: Pool,
}

impl BlobData {
    fn new(data: Vec<u8>, pool: Pool) -> Result<Self> {
        let data = Pin::new(data);
        let data_slice: &[u8] = data.as_ref().get_ref();
        let static_data_slice: &'static [u8] = unsafe { std::mem::transmute(data_slice) };
        let mut reader = BytesReader::from_bytes(static_data_slice);
        let blob = PbfBlob::from_reader(&mut reader, static_data_slice)?;
        Ok(Self { blob, data, pool })
    }

    fn set_raw(&mut self, data: Vec<u8>) -> &[u8] {
        let data = Pin::new(data);
        let data_slice: &[u8] = data.as_ref().get_ref();
        let static_data_slice: &'static [u8] = unsafe { std::mem::transmute(data_slice) };
        self.blob = PbfBlob {
            raw_size: None,
            data: PbfBlobData::raw(std::borrow::Cow::Borrowed(&static_data_slice)),
        };
        let old_data = Pin::into_inner(std::mem::replace(&mut self.data, data));
        self.pool.push(old_data);
        static_data_slice
    }

    fn new_buf_with_raw_size(&self) -> Vec<u8> {
        let mut buf = self.pool.get();
        if let Some(s) = self.blob.raw_size {
            if s > 0 {
                buf.reserve_exact(s as usize);
            }
        }
        buf
    }

    pub fn decode(&mut self) -> Result<&[u8]> {
        match self.blob.data {
            PbfBlobData::None => Ok(&[]),
            PbfBlobData::raw(ref d) => Ok(d),
            #[cfg(feature = "zlib")]
            PbfBlobData::zlib_data(ref d) => {
                let cursor = io::Cursor::new(d);
                let mut decoder = flate2::bufread::ZlibDecoder::new(cursor);
                let mut buf = self.new_buf_with_raw_size();
                decoder.read_to_end(&mut buf)?;
                Ok(self.set_raw(buf))
            }
            #[cfg(feature = "lzma")]
            PbfBlobData::lzma_data(ref d) => {
                let cursor = io::Cursor::new(d);
                let mut decoder = xz2::bufread::XzDecoder::new(cursor);
                let mut buf = self.new_buf_with_raw_size();
                decoder.read_to_end(&mut buf)?;
                Ok(self.set_raw(buf))
            }
            _ => Err(Error::UnsupportedEncoding),
        }
    }
}

impl Drop for BlobData {
    fn drop(&mut self) {
        let buf = Pin::into_inner(std::mem::replace(&mut self.data, Pin::new(Vec::new())));
        self.pool.push(buf);
    }
}

pub struct Blob<B> {
    data: BlobData,
    phantom: PhantomData<fn(B)>,
}

impl<B: Block> Blob<B> {
    #[inline]
    pub fn read(&mut self) -> Result<B::Target<'_>> {
        let data = self.data.decode()?;
        let mut reader = BytesReader::from_bytes(&data);
        let msg = B::Target::from_reader(&mut reader, &data)?;
        Ok(msg)
    }
}

pub trait Block {
    type Target<'a>: MessageRead<'a>;
}

#[derive(Clone)]
struct Pool(Arc<ArrayQueue<Vec<u8>>>);

impl Pool {
    fn new() -> Pool {
        Self(Arc::new(ArrayQueue::new(32)))
    }
    fn get(&self) -> Vec<u8> {
        if let Some(mut buf) = self.0.pop() {
            buf.clear();
            buf
        } else {
            Vec::new()
        }
    }
    fn push(&self, buf: Vec<u8>) {
        if buf.capacity() > 0 {
            let _ = self.0.push(buf);
        }
    }
}

pub struct Blobs<R> {
    reader: R,
    hdr_buf: Vec<u8>,
    pool: Pool,
}

impl<R> Blobs<R> {
    #[inline]
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: AsRef<[u8]>> Blobs<io::Cursor<R>> {
    #[inline]
    pub fn from_bytes(bytes: R) -> Self {
        Self {
            reader: io::Cursor::new(bytes),
            hdr_buf: Vec::new(),
            pool: Pool::new(),
        }
    }
}

impl<R: io::Seek> Blobs<R> {
    #[inline]
    pub fn rewind(&mut self) -> io::Result<()> {
        self.reader.rewind()?;
        Ok(())
    }
}

fn read_bytes_exact<R: io::Read>(
    reader: &mut R,
    bytes: &mut Vec<u8>,
    exact_size: usize,
) -> Result<()> {
    bytes.reserve(exact_size);
    let len = reader.take(exact_size as u64).read_to_end(bytes)?;
    if len != exact_size {
        Err(io::ErrorKind::UnexpectedEof.into())
    } else {
        Ok(())
    }
}

impl<R: io::Read> Blobs<R> {
    #[inline]
    pub fn from_read(reader: R) -> Self {
        Self {
            reader,
            hdr_buf: Vec::new(),
            pool: Pool::new(),
        }
    }

    pub fn header(&mut self) -> Result<OSMHeaderBlob> {
        match self.next_blob()? {
            Some((BlobType::OSMHeader, data)) => Ok(Blob {
                data,
                phantom: PhantomData,
            }),
            Some((blob_type, _)) => Err(Error::UnexpectedBlobType(blob_type)),
            None => Err(std::io::ErrorKind::UnexpectedEof.into()),
        }
    }

    fn read_bytes_exact(&mut self, exact_size: usize) -> Result<Vec<u8>> {
        let mut bytes = self.pool.get();
        read_bytes_exact(&mut self.reader, &mut bytes, exact_size)?;
        Ok(bytes)
    }

    fn next_blob(&mut self) -> Result<Option<(BlobType, BlobData)>> {
        let header_size = match self.reader.read_u32::<BigEndian>() {
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(None); // Expected EOF
            }
            Err(e) => return Err(Error::IoError(e)),
            Ok(header_size) if header_size > MAX_HEADER_SIZE => {
                return Err(Error::BlobHeaderToLarge);
            }
            Ok(header_size) => header_size as usize,
        };
        self.hdr_buf.clear();
        read_bytes_exact(&mut self.reader, &mut self.hdr_buf, header_size)?;

        let mut reader = BytesReader::from_bytes(&self.hdr_buf);
        let header = PbfBlobHeader::from_reader(&mut reader, &self.hdr_buf)?;

        let blob_type = BlobType::from_str(&header.type_pb);
        if matches!(blob_type, BlobType::Other(_)) {
            return Err(Error::UnexpectedBlobType(blob_type));
        };

        let data_size = header.datasize as usize;
        if data_size > MAX_UNCOMPRESSED_DATA_SIZE {
            return Err(Error::BlobDataToLarge);
        }

        let data = self.read_bytes_exact(data_size)?;

        Ok(Some((blob_type, BlobData::new(data, self.pool.clone())?)))
    }
}

impl<R: io::Read> iter::Iterator for Blobs<R> {
    type Item = Result<OSMDataBlob>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.next_blob() {
                Err(e) => {
                    return Some(Err(e));
                }
                Ok(None) => {
                    return None;
                }
                Ok(Some((BlobType::OSMData, data))) => {
                    return Some(Ok(Blob {
                        data,
                        phantom: PhantomData,
                    }));
                }
                // skip unsupported blobs and header-blobs
                _ => {}
            }
        }
    }
}

impl<R: io::BufRead> iter::FusedIterator for Blobs<R> {}
