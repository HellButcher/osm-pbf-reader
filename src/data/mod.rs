use std::string::FromUtf8Error;

use crate::{blob::Block, error::Result};

pub use osm_pbf_proto::osmformat::{Info as PbfInfo, PrimitiveBlock as PbfPrimitiveBlock};

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
            version: info.version.unwrap_or(0) as u32,
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

impl Block for PrimitiveBlock {
    type Message = PbfPrimitiveBlock;

    #[inline]
    fn from_message(pbf: PbfPrimitiveBlock) -> Result<Self> {
        let strings = pbf.stringtable.s.into_iter()
                .map(String::from_utf8)
                .collect::<Result<Vec<String>, FromUtf8Error>>()?;
        Ok(Self {
            strings,
            offset: Offset {
                lat: pbf.lat_offset.unwrap_or(0),
                lon: pbf.lon_offset.unwrap_or(0),
                granularity: pbf.granularity.unwrap_or(100),
            },
            primitive_groups: pbf.primitivegroup,
        })
    }
}

pub type OSMDataBlob = crate::blob::Blob<PrimitiveBlock>;
