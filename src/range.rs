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
pub struct Range<'a, T: ?Sized> {
    inner: &'a T,
    offset: usize,
    len: usize,
}

impl<T: ?Sized> Copy for Range<'_, T> {}

impl<T: ?Sized> Clone for Range<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Range<'_, T> {
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn inner(&self) -> &T {
        self.inner
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
            inner: self.inner,
            offset: self.offset + offset,
            len,
        }
    }

    pub fn get(&self, bounds: impl RangeBounds) -> Option<Self> {
        let (offset, len) = bounds.try_index(self.len)?;
        Some(Range {
            inner: self.inner,
            offset: self.offset + offset,
            len,
        })
    }

    pub fn split_at(&self, mid: usize) -> (Self, Self) {
        (self.slice(..mid), self.slice(mid..))
    }
}

impl<'a, T> Range<'a, [T]> {
    pub fn new(inner: &'a [T], bounds: impl RangeBounds) -> Self {
        let (offset, len) = bounds.index(inner.len());
        Range { inner, offset, len }
    }

    pub fn empty() -> Self {
        Range {
            inner: &[],
            offset: 0,
            len: 0,
        }
    }

    pub fn as_slice(&self) -> &'a [T] {
        &self.inner[self.offset..self.offset + self.len]
    }

    pub fn iter(&self) -> std::slice::Iter<'a, T> {
        self.as_slice().iter()
    }
}

impl<'a, T> Range<'a, [T]>
where
    T: PartialEq,
{
    pub fn common_prefix_len(&self, other: Range<'_, [T]>) -> usize {
        for (i, (item1, item2)) in self.iter().zip(other.iter()).enumerate() {
            if item1 != item2 {
                return i;
            }
        }
        cmp::min(self.len, other.len)
    }

    pub fn common_suffix_len(&self, other: Range<'_, [T]>) -> usize {
        for (i, (item1, item2)) in self.iter().rev().zip(other.iter().rev()).enumerate() {
            if item1 != item2 {
                return i;
            }
        }
        cmp::min(self.len, other.len)
    }
}

impl<'a> Range<'a, str> {
    pub fn new_str(inner: &'a str, bounds: impl RangeBounds) -> Self {
        let (offset, len) = bounds.index(inner.len());
        Range { inner, offset, len }
    }

    pub fn empty_str() -> Self {
        Range {
            inner: "",
            offset: 0,
            len: 0,
        }
    }

    pub fn as_str(&self) -> &'a str {
        &self.inner[self.offset..self.offset + self.len]
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
