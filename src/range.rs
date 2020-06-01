use std::{cmp, fmt::Debug, ops};

// TODO try and have range be generic across &[T] and &str
// impl<'a, T> Range<'a, [T]> {
//     //
// }

// impl<'a> Range<'a, str> {
//     //
// }

// Range type inspired by the Range type used in [dissimilar](https://docs.rs/dissimilar)
#[derive(Debug)]
pub struct Range<'a, T> {
    text: &'a [T],
    offset: usize,
    len: usize,
}

impl<T> Copy for Range<'_, T> {}

impl<T> Clone for Range<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Range<'a, T> {
    pub fn new(text: &'a [T], bounds: impl RangeBounds) -> Self {
        let (offset, len) = bounds.index(text.len());
        Range { text, offset, len }
    }

    pub fn empty() -> Self {
        Range {
            text: &[],
            offset: 0,
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn slice(&self, bounds: impl RangeBounds) -> Self {
        let (offset, len) = bounds.index(self.len);
        Range {
            text: self.text,
            offset: self.offset + offset,
            len,
        }
    }

    pub fn get(&self, bounds: impl RangeBounds) -> Option<Self> {
        let (offset, len) = bounds.try_index(self.len)?;
        Some(Range {
            text: self.text,
            offset: self.offset + offset,
            len,
        })
    }

    pub fn split_at(&self, mid: usize) -> (Self, Self) {
        (self.slice(..mid), self.slice(mid..))
    }

    pub fn as_slice(&self) -> &'a [T] {
        &self.text[self.offset..self.offset + self.len]
    }

    pub fn iter(&self) -> std::slice::Iter<'a, T> {
        self.as_slice().iter()
    }
}

impl<'a, T> Range<'a, T>
where
    T: PartialEq,
{
    pub fn common_prefix_len(&self, other: Range<'_, T>) -> usize {
        for (i, (item1, item2)) in self.iter().zip(other.iter()).enumerate() {
            if item1 != item2 {
                return i;
            }
        }
        cmp::min(self.len, other.len)
    }

    pub fn common_suffix_len(&self, other: Range<'_, T>) -> usize {
        for (i, (item1, item2)) in self.iter().rev().zip(other.iter().rev()).enumerate() {
            if item1 != item2 {
                return i;
            }
        }
        cmp::min(self.len, other.len)
    }
}

pub trait RangeBounds: Sized + Clone + Debug {
    // Returns (offset, len).
    fn try_index(self, len: usize) -> Option<(usize, usize)>;

    fn index(self, len: usize) -> (usize, usize) {
        match self.clone().try_index(len) {
            Some(range) => range,
            None => panic!("index out of range, index={:?}, len={}", self, len),
        }
    }
}

impl RangeBounds for ops::Range<usize> {
    fn try_index(self, len: usize) -> Option<(usize, usize)> {
        if self.start <= self.end && self.end <= len {
            Some((self.start, self.end - self.start))
        } else {
            None
        }
    }
}

impl RangeBounds for ops::RangeFrom<usize> {
    fn try_index(self, len: usize) -> Option<(usize, usize)> {
        if self.start <= len {
            Some((self.start, len - self.start))
        } else {
            None
        }
    }
}

impl RangeBounds for ops::RangeTo<usize> {
    fn try_index(self, len: usize) -> Option<(usize, usize)> {
        if self.end <= len {
            Some((0, self.end))
        } else {
            None
        }
    }
}

impl RangeBounds for ops::RangeFull {
    fn try_index(self, len: usize) -> Option<(usize, usize)> {
        Some((0, len))
    }
}
