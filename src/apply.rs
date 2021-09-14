use crate::{
    patch::{Hunk, Line, Patch},
    utils::LineIter,
};
use std::collections::VecDeque;
use std::{fmt, iter};

/// An error returned when [`apply`]ing a `Patch` fails
///
/// [`apply`]: fn.apply.html
#[derive(Debug)]
pub struct ApplyError(usize);

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error applying hunk #{}", self.0)
    }
}

impl std::error::Error for ApplyError {}

#[derive(Debug)]
enum ImageLine<'a, T: ?Sized> {
    Unpatched(&'a T),
    Patched(&'a T),
}

impl<'a, T: ?Sized> ImageLine<'a, T> {
    fn inner(&self) -> &'a T {
        match self {
            ImageLine::Unpatched(inner) | ImageLine::Patched(inner) => inner,
        }
    }

    fn into_inner(self) -> &'a T {
        self.inner()
    }

    fn is_patched(&self) -> bool {
        match self {
            ImageLine::Unpatched(_) => false,
            ImageLine::Patched(_) => true,
        }
    }
}

impl<T: ?Sized> Copy for ImageLine<'_, T> {}

impl<T: ?Sized> Clone for ImageLine<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Debug)]
pub struct ApplyOptions {
    max_fuzzy: usize,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        ApplyOptions::new()
    }
}

impl ApplyOptions {
    pub fn new() -> Self {
        ApplyOptions { max_fuzzy: 0 }
    }

    pub fn with_max_fuzzy(mut self, max_fuzzy: usize) -> Self {
        self.max_fuzzy = max_fuzzy;
        self
    }
}

/// Apply a `Patch` to a base image
///
/// ```
/// use diffy::{apply, Patch};
///
/// let s = "\
/// --- a/ideals
/// +++ b/ideals
/// @@ -1,4 +1,6 @@
///  First:
///      Life before death,
///      strength before weakness,
///      journey before destination.
/// +Second:
/// +    I will protect those who cannot protect themselves.
/// ";
///
/// let patch = Patch::from_str(s).unwrap();
///
/// let base_image = "\
/// First:
///     Life before death,
///     strength before weakness,
///     journey before destination.
/// ";
///
/// let expected = "\
/// First:
///     Life before death,
///     strength before weakness,
///     journey before destination.
/// Second:
///     I will protect those who cannot protect themselves.
/// ";
///
/// assert_eq!(apply(base_image, &patch).unwrap(), expected);
/// ```
pub fn apply(base_image: &str, patch: &Patch<'_, str>) -> Result<String, ApplyError> {
    let mut image: Vec<_> = LineIter::new(base_image)
        .map(ImageLine::Unpatched)
        .collect();

    for (i, hunk) in patch.hunks().iter().enumerate() {
        apply_hunk(&mut image, hunk, &ApplyOptions::new()).map_err(|_| ApplyError(i + 1))?;
    }

    Ok(image.into_iter().map(ImageLine::into_inner).collect())
}

/// Apply a non-utf8 `Patch` to a base image
pub fn apply_bytes(base_image: &[u8], patch: &Patch<'_, [u8]>) -> Result<Vec<u8>, ApplyError> {
    let mut image: Vec<_> = LineIter::new(base_image)
        .map(ImageLine::Unpatched)
        .collect();

    for (i, hunk) in patch.hunks().iter().enumerate() {
        apply_hunk(&mut image, hunk, &ApplyOptions::new()).map_err(|_| ApplyError(i + 1))?;
    }

    Ok(image
        .into_iter()
        .flat_map(ImageLine::into_inner)
        .copied()
        .collect())
}

/// Try applying all hunks a `Patch` to a base image
pub fn apply_all_bytes(
    base_image: &[u8],
    patch: &Patch<'_, [u8]>,
    options: ApplyOptions,
) -> (Vec<u8>, Vec<usize>) {
    let mut image: Vec<_> = LineIter::new(base_image)
        .map(ImageLine::Unpatched)
        .collect();

    let mut failed_indices = Vec::new();

    for (i, hunk) in patch.hunks().iter().enumerate() {
        if let Some(_) = apply_hunk(&mut image, hunk, &options).err() {
            failed_indices.push(i);
        }
    }

    (
        image
            .into_iter()
            .flat_map(ImageLine::into_inner)
            .copied()
            .collect(),
        failed_indices,
    )
}

/// Try applying all hunks a `Patch` to a base image
pub fn apply_all(
    base_image: &str,
    patch: &Patch<'_, str>,
    options: ApplyOptions,
) -> (String, Vec<usize>) {
    let mut image: Vec<_> = LineIter::new(base_image)
        .map(ImageLine::Unpatched)
        .collect();

    let mut failed_indices = Vec::new();

    for (i, hunk) in patch.hunks().iter().enumerate() {
        if let Some(_) = apply_hunk(&mut image, hunk, &options).err() {
            failed_indices.push(i);
        }
    }

    (
        image.into_iter().map(ImageLine::into_inner).collect(),
        failed_indices,
    )
}

fn apply_hunk<'a, T: PartialEq + ?Sized>(
    image: &mut Vec<ImageLine<'a, T>>,
    hunk: &Hunk<'a, T>,
    options: &ApplyOptions,
) -> Result<(), ()> {
    // Find position

    let max_fuzzy = pre_context_line_count(hunk.lines())
        .min(post_context_line_count(hunk.lines()))
        .min(options.max_fuzzy);
    let (pos, fuzzy) = find_position(image, hunk, max_fuzzy).ok_or(())?;
    let begin = pos + fuzzy;
    let end = pos
        + pre_image_line_count(hunk.lines())
            .checked_sub(fuzzy)
            .unwrap_or(0);

    // update image
    image.splice(
        begin..end,
        skip_last(post_image(hunk.lines()).skip(fuzzy), fuzzy).map(ImageLine::Patched),
    );

    Ok(())
}

// Search in `image` for a palce to apply hunk.
// This follows the general algorithm (minus fuzzy-matching context lines) described in GNU patch's
// man page.
//
// It might be worth looking into other possible positions to apply the hunk to as described here:
// https://neil.fraser.name/writing/patch/
fn find_position<T: PartialEq + ?Sized>(
    image: &[ImageLine<T>],
    hunk: &Hunk<'_, T>,
    max_fuzzy: usize,
) -> Option<(usize, usize)> {
    let pos = hunk.new_range().start().saturating_sub(1);

    for fuzzy in 0..=max_fuzzy {
        // Create an iterator that starts with 'pos' and then interleaves
        // moving pos backward/foward by one.
        let backward = (0..pos).rev();
        let forward = pos + 1..image.len();
        for pos in iter::once(pos).chain(interleave(backward, forward)) {
            if match_fragment(image, hunk.lines(), pos, fuzzy) {
                return Some((pos, fuzzy));
            }
        }
    }

    None
}

fn pre_context_line_count<T: ?Sized>(lines: &[Line<'_, T>]) -> usize {
    lines
        .iter()
        .take_while(|x| matches!(x, Line::Context(_)))
        .count()
}

fn post_context_line_count<T: ?Sized>(lines: &[Line<'_, T>]) -> usize {
    lines
        .iter()
        .rev()
        .take_while(|x| matches!(x, Line::Context(_)))
        .count()
}

fn pre_image_line_count<T: ?Sized>(lines: &[Line<'_, T>]) -> usize {
    pre_image(lines).count()
}

fn post_image<'a, 'b, T: ?Sized>(lines: &'b [Line<'a, T>]) -> impl Iterator<Item = &'a T> + 'b {
    lines.iter().filter_map(|line| match line {
        Line::Context(l) | Line::Insert(l) => Some(*l),
        Line::Delete(_) => None,
    })
}

fn pre_image<'a, 'b, T: ?Sized>(lines: &'b [Line<'a, T>]) -> impl Iterator<Item = &'a T> + 'b {
    lines.iter().filter_map(|line| match line {
        Line::Context(l) | Line::Delete(l) => Some(*l),
        Line::Insert(_) => None,
    })
}

fn match_fragment<T: PartialEq + ?Sized>(
    image: &[ImageLine<T>],
    lines: &[Line<'_, T>],
    pos: usize,
    fuzzy: usize,
) -> bool {
    let len = pre_image_line_count(lines);
    let begin = pos + fuzzy;
    let end = pos + len.checked_sub(fuzzy).unwrap_or(0);

    let image = if let Some(image) = image.get(begin..end) {
        image
    } else {
        return false;
    };

    // If any of these lines have already been patched then we can't match at this position
    if image.iter().any(ImageLine::is_patched) {
        return false;
    }

    pre_image(&lines[fuzzy..len - fuzzy]).eq(image.iter().map(ImageLine::inner))
}

#[derive(Debug)]
struct Interleave<I, J> {
    a: iter::Fuse<I>,
    b: iter::Fuse<J>,
    flag: bool,
}

fn interleave<I, J>(
    i: I,
    j: J,
) -> Interleave<<I as IntoIterator>::IntoIter, <J as IntoIterator>::IntoIter>
where
    I: IntoIterator,
    J: IntoIterator<Item = I::Item>,
{
    Interleave {
        a: i.into_iter().fuse(),
        b: j.into_iter().fuse(),
        flag: false,
    }
}

impl<I, J> Iterator for Interleave<I, J>
where
    I: Iterator,
    J: Iterator<Item = I::Item>,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        self.flag = !self.flag;
        if self.flag {
            match self.a.next() {
                None => self.b.next(),
                item => item,
            }
        } else {
            match self.b.next() {
                None => self.a.next(),
                item => item,
            }
        }
    }
}

fn skip_last<I: Iterator>(iter: I, count: usize) -> SkipLast<I, I::Item> {
    SkipLast {
        iter: iter.fuse(),
        buffer: VecDeque::with_capacity(count),
        count,
    }
}

#[derive(Debug)]
struct SkipLast<Iter: Iterator<Item = Item>, Item> {
    iter: iter::Fuse<Iter>,
    buffer: VecDeque<Item>,
    count: usize,
}

impl<Iter: Iterator<Item = Item>, Item> Iterator for SkipLast<Iter, Item> {
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            return self.iter.next();
        }
        while self.buffer.len() != self.count {
            self.buffer.push_front(self.iter.next()?);
        }
        let next = self.iter.next()?;
        let res = self.buffer.pop_back()?;
        self.buffer.push_front(next);
        Some(res)
    }
}

#[cfg(test)]
mod skip_last_test {
    use crate::apply::skip_last;

    #[test]
    fn skip_last_test() {
        let a = [1, 2, 3, 4, 5, 6, 7];

        assert_eq!(
            skip_last(a.iter().copied(), 0)
                .collect::<Vec<_>>()
                .as_slice(),
            &[1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(
            skip_last(a.iter().copied(), 5)
                .collect::<Vec<_>>()
                .as_slice(),
            &[1, 2]
        );
        assert_eq!(
            skip_last(a.iter().copied(), 7)
                .collect::<Vec<_>>()
                .as_slice(),
            &[]
        );
    }
}
