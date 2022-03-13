use std::ops::Deref;

pub use osm_pbf_proto::osmformat::Way as PbfWay;

use super::{tags::Tags, Meta};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WayId(pub i64);

pub struct Way<'l> {
    pub id: WayId,

    tags: Tags<'l>,
    meta: Meta,
}

impl Deref for Way<'_> {
    type Target = Meta;
    #[inline]
    fn deref(&self) -> &Meta {
        &self.meta
    }
}

impl<'l> Way<'l> {
    #[inline]
    pub(crate) fn from_pbf(w: &'l PbfWay, strings: &'l [String]) -> Self {
        Self {
            id: WayId(w.id),
            tags: Tags::new(strings, &w.keys, &w.vals),
            meta: w.info.as_ref().map(Meta::from_info).unwrap_or_default(),
        }
    }

    pub fn tags(&self) -> Tags<'l> {
        self.tags
    }
}
