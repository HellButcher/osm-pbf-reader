use std::{iter::FusedIterator, ops::Deref};

#[derive(Copy, Clone)]
enum TagLayout<'l> {
    Normal(&'l [u32], &'l [u32]),
    Dense(&'l [i32]),
}

#[derive(Copy, Clone)]
pub struct Tags<'l> {
    strings: &'l [String],
    tags: TagLayout<'l>,
}

impl<'l> Tags<'l> {
    #[inline(always)]
    pub(crate) fn new(strings: &'l [String], keys: &'l [u32], values: &'l [u32]) -> Tags<'l> {
        Tags {
            strings,
            tags: TagLayout::Normal(keys, values),
        }
    }

    #[inline(always)]
    pub(crate) fn new_dense(strings: &'l [String], key_values: &'l [i32]) -> Tags<'l> {
        Tags {
            strings,
            tags: TagLayout::Dense(key_values),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self.tags {
            TagLayout::Normal(keys, values) => keys.len().min(values.len()),
            TagLayout::Dense(key_values) => key_values.len() / 2,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<(&'l str, &'l str)> {
        let key_index;
        let value_index;
        match self.tags {
            TagLayout::Normal(keys, values) => {
                key_index = keys.get(index).copied()? as usize;
                value_index = values.get(index).copied()? as usize;
            }
            TagLayout::Dense(key_values) => {
                let tmp = index / 2;
                key_index = key_values.get(tmp).copied()? as usize;
                value_index = key_values.get(tmp + 1).copied()? as usize;
            }
        }
        let key = self.strings.get(key_index).map(Deref::deref).unwrap_or("");
        let value = self
            .strings
            .get(value_index)
            .map(Deref::deref)
            .unwrap_or("");
        Some((key, value))
    }

    #[inline]
    pub fn iter(&self) -> TagsIter<'l> {
        TagsIter {
            tags: *self,
            pos: 0,
        }
    }
}

impl<'l> IntoIterator for Tags<'l> {
    type Item = (&'l str, &'l str);
    type IntoIter = TagsIter<'l>;
    fn into_iter(self) -> TagsIter<'l> {
        TagsIter { tags: self, pos: 0 }
    }
}

impl<'l> IntoIterator for &'_ Tags<'l> {
    type Item = (&'l str, &'l str);
    type IntoIter = TagsIter<'l>;
    fn into_iter(self) -> TagsIter<'l> {
        TagsIter {
            tags: *self,
            pos: 0,
        }
    }
}

pub struct TagsIter<'l> {
    tags: Tags<'l>,
    pos: usize,
}

impl<'l> Deref for TagsIter<'l> {
    type Target = Tags<'l>;
    fn deref(&self) -> &Tags<'l> {
        &self.tags
    }
}

impl<'l> Iterator for TagsIter<'l> {
    type Item = (&'l str, &'l str);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let r = self.tags.get(self.pos)?;
        self.pos += 1;
        Some(r)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.tags.len() - self.pos;
        (len, Some(len))
    }

    #[inline]
    fn count(self) -> usize {
        self.tags.len() - self.pos
    }

    #[inline]
    fn last(self) -> Option<Self::Item> {
        let len = self.tags.len();
        if len > 0 {
            self.tags.get(len - 1)
        } else {
            None
        }
    }
    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;
        self.next()
    }
}
impl<'l> FusedIterator for TagsIter<'l> {}
