use std::{borrow::Cow, str::Utf8Error};

use crate::error::Result;

use osm_pbf_proto::{
    osmformat::{
        Info as PbfInfo, PrimitiveBlock as PbfPrimitiveBlock, PrimitiveGroup as PbfPrimitiveGroup,
    },
    quick_protobuf as qpb,
};

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
    fn from_info(info: Option<&PbfInfo>) -> Self {
        match info {
            Some(info) => Self {
                version: info.version as u32,
                visible: info.visible.unwrap_or(true),
            },
            None => Self {
                version: 0,
                visible: true,
            },
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

#[derive(Copy, Clone, Default, Debug, PartialEq)]
struct Offset {
    pub lat: i64,
    pub lon: i64,
    pub granularity: i32,
}

#[derive(Copy, Clone, Default)]
struct DenseState {
    id: i64,
    lat: i64,
    lon: i64,
    kv_pos: usize,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct PrimitiveBlock<'a> {
    pub strings: Vec<Cow<'a, str>>,
    pub primitive_groups: Vec<PbfPrimitiveGroup>,
    offset: Offset,
}

impl<'a> qpb::MessageRead<'a> for PrimitiveBlock<'a> {
    fn from_reader(r: &mut qpb::BytesReader, bytes: &'a [u8]) -> qpb::Result<Self> {
        let msg = PbfPrimitiveBlock::from_reader(r, bytes)?;
        let msg = PrimitiveBlock::try_from(msg)?;
        Ok(msg)
    }
}

fn cow_bytes_to_str<'a>(b: Cow<'a, [u8]>) -> Result<Cow<'a, str>, Utf8Error> {
    match b {
        Cow::Borrowed(b) => Ok(Cow::Borrowed(std::str::from_utf8(b)?)),
        Cow::Owned(b) => Ok(Cow::Owned(
            String::from_utf8(b).map_err(|e| e.utf8_error())?,
        )),
    }
}

impl<'a> TryFrom<PbfPrimitiveBlock<'a>> for PrimitiveBlock<'a> {
    type Error = Utf8Error;
    fn try_from(pbf: PbfPrimitiveBlock<'a>) -> Result<Self, Utf8Error> {
        let strings: Vec<_> = pbf
            .stringtable
            .s
            .into_iter()
            .map(cow_bytes_to_str)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            strings,
            primitive_groups: pbf.primitivegroup,
            offset: Offset {
                lat: pbf.lat_offset,
                lon: pbf.lon_offset,
                granularity: pbf.granularity,
            },
        })
    }
}

#[doc(hidden)]
pub struct DataBlobMarker;

impl crate::blob::Block for DataBlobMarker {
    type Target<'a> = PrimitiveBlock<'a>;
}

pub type OSMDataBlob = crate::blob::Blob<DataBlobMarker>;
