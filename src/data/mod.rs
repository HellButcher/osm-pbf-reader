use std::{borrow::Cow, str::Utf8Error};

use crate::{blob::Block, error::Result};

pub use osm_pbf_proto::osmformat::{Info as PbfInfo, PrimitiveBlock as PbfPrimitiveBlock};
use osm_pbf_proto::quick_protobuf::{BytesReader, MessageRead};

pub mod changeset;
pub mod node;
pub mod primitive;
pub mod primitive_group;
pub mod relation;
pub mod tags;
pub mod way;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Meta {
    pub version: u32,
    pub visible: bool,
}

impl Meta {
    fn from_info(info: &PbfInfo) -> Self {
        Self {
            version: if info.version == -1 {
                0
            } else {
                info.version as u32
            },
            visible: info.visible.unwrap_or(true),
        }
    }
}

impl Default for Meta {
    #[inline(always)]
    fn default() -> Self {
        Self {
            version: 0,
            visible: true,
        }
    }
}

#[derive(Copy, Clone)]
struct Offset {
    lat: i64,
    lon: i64,
    granularity: i32,
}

#[derive(Copy, Clone, Default)]
struct DenseState {
    id: i64,
    lat: i64,
    lon: i64,
    kv_pos: usize,
}

#[derive(Clone)]
pub struct PrimitiveBlock {
    strings: Vec<String>,
    primitive_groups: Vec<primitive::PbfPrimitiveGroup>,
    offset: Offset,
}

fn from_utf8(bytes: Cow<'_, [u8]>) -> Result<String, Utf8Error> {
    match bytes {
        Cow::Owned(vec) => Ok(String::from_utf8(vec).map_err(|e| e.utf8_error())?),
        Cow::Borrowed(slice) => Ok(std::str::from_utf8(slice)?.to_owned()),
    }
}

impl PrimitiveBlock {
    #[inline]
    fn from_message(pbf: PbfPrimitiveBlock<'_>) -> Result<Self> {
        let strings = pbf
            .stringtable
            .s
            .into_iter()
            .map(from_utf8)
            .collect::<Result<Vec<String>, Utf8Error>>()?;
        Ok(Self {
            strings,
            offset: Offset {
                lat: pbf.lat_offset,
                lon: pbf.lon_offset,
                granularity: pbf.granularity,
            },
            primitive_groups: pbf.primitivegroup,
        })
    }
}

impl Block for PrimitiveBlock {
    #[inline]
    fn read_from_bytes(bytes: &[u8]) -> Result<Self> {
        let msg = PbfPrimitiveBlock::from_reader(&mut BytesReader::from_bytes(bytes), bytes)?;
        Self::from_message(msg)
    }
}

pub type OSMDataBlob = crate::blob::Blob<PrimitiveBlock>;
