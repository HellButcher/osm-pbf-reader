use std::ops::Deref;

pub use osm_pbf_proto::osmformat::Relation as PbfRelation;

use super::{tags::Tags, Meta};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelationId(pub i64);

pub struct Relation<'l> {
    pub id: RelationId,

    tags: Tags<'l>,
    meta: Meta,
}

impl Deref for Relation<'_> {
    type Target = Meta;
    #[inline]
    fn deref(&self) -> &Meta {
        &self.meta
    }
}

impl<'l> Relation<'l> {
    #[inline]
    pub(crate) fn from_pbf(r: &'l PbfRelation, strings: &'l [String]) -> Self {
        Self {
            id: RelationId(r.id),
            tags: Tags::new(strings, &r.keys, &r.vals),
            meta: r.info.as_ref().map(Meta::from_info).unwrap_or_default(),
        }
    }

    pub fn tags(&self) -> Tags<'l> {
        self.tags
    }
}
