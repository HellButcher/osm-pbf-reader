use std::borrow::Cow;

pub use osm_pbf_proto::osmformat::HeaderBlock as PbfHeaderBlock;
use osm_pbf_proto::quick_protobuf::{BytesReader, MessageRead};

use crate::{blob::Block, error::Result};

// REQUIRED FEATURES
pub const DENSE_NODES: &str = "DenseNodes";
pub const HISTORICAL_INFORMATION: &str = "HistoricalInformation";

// OPTIONAL FEATURES
pub const HAS_METADATA: &str = "Has_Metadata";
pub const SORT_TYPE_THEN_ID: &str = "Sort.Type_then_ID";
pub const SORT_GEOGRAPHIC: &str = "Sort.Geographic";
pub const LOCATIONS_ON_WAYS: &str = "LocationsOnWays";

pub struct HeaderBlock {
    pub bbox: Option<osm_pbf_proto::osmformat::HeaderBBox>,
    pub required_features: Vec<String>,
    pub optional_features: Vec<String>,
    pub writingprogram: Option<String>,
    pub source: Option<String>,
    pub osmosis_replication_timestamp: Option<i64>,
    pub osmosis_replication_sequence_number: Option<i64>,
    pub osmosis_replication_base_url: Option<String>,
}

impl HeaderBlock {
    #[inline]
    pub fn from_message(pbf: PbfHeaderBlock<'_>) -> Result<Self> {
        Ok(Self {
            bbox: pbf.bbox,
            required_features: pbf
                .required_features
                .into_iter()
                .map(Cow::into_owned)
                .collect(),
            optional_features: pbf
                .optional_features
                .into_iter()
                .map(Cow::into_owned)
                .collect(),
            writingprogram: pbf.writingprogram.map(Cow::into_owned),
            source: pbf.source.map(Cow::into_owned),
            osmosis_replication_timestamp: pbf.osmosis_replication_timestamp,
            osmosis_replication_sequence_number: pbf.osmosis_replication_sequence_number,
            osmosis_replication_base_url: pbf.osmosis_replication_base_url.map(Cow::into_owned),
        })
    }
}

impl Block for HeaderBlock {
    #[inline]
    fn read_from_bytes(bytes: &[u8]) -> Result<Self> {
        let msg = PbfHeaderBlock::from_reader(&mut BytesReader::from_bytes(bytes), bytes)?;
        Self::from_message(msg)
    }
}

pub type OSMHeaderBlob<'l> = crate::blob::Blob<PbfHeaderBlock<'l>>;
