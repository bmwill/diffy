use crate::{
    patch::{Hunk, Line, Patch},
    utils::LineIter,
};
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

#[derive(Copy, Clone, Debug)]
enum ImageLine<'a> {
    Unpatched(&'a str),
    Patched(&'a str),
}

impl<'a> ImageLine<'a> {
    fn inner(&self) -> &'a str {
        match self {
            ImageLine::Unpatched(inner) | ImageLine::Patched(inner) => inner,
        }
    }

    fn into_inner(self) -> &'a str {
        self.inner()
    }

    fn is_patched(&self) -> bool {
        match self {
            ImageLine::Unpatched(_) => false,
            ImageLine::Patched(_) => true,
        }
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
pub fn apply(base_image: &str, patch: &Patch<'_>) -> Result<String, ApplyError> {
    let mut image: Vec<_> = LineIter::new(base_image)
        .map(ImageLine::Unpatched)
        .collect();

    for (i, hunk) in patch.hunks().iter().enumerate() {
        apply_hunk(&mut image, hunk).map_err(|_| ApplyError(i + 1))?;
    }

    Ok(image.into_iter().map(ImageLine::into_inner).collect())
}

fn apply_hunk<'a>(image: &mut Vec<ImageLine<'a>>, hunk: &Hunk<'a>) -> Result<(), ()> {
    // Find position
    let pos = find_position(image, hunk).ok_or(())?;

    // update image
    image.splice(
        pos..pos + pre_image_line_count(hunk.lines()),
        post_image(hunk.lines()).map(ImageLine::Patched),
    );

    Ok(())
}

// Search in `image` for a palce to apply hunk.
// This follows the general algorithm (minus fuzzy-matching context lines) described in GNU patch's
// man page.
//
// It might be worth looking into other possible positions to apply the hunk to as described here:
// https://neil.fraser.name/writing/patch/
fn find_position(image: &[ImageLine], hunk: &Hunk<'_>) -> Option<usize> {
    let pos = hunk.new_range().start().saturating_sub(1);

    // Create an iterator that starts with 'pos' and then interleaves
    // moving pos backward/foward by one.
    let backward = (0..pos).rev();
    let forward = pos + 1..image.len();
    for pos in iter::once(pos).chain(interleave(backward, forward)) {
        if match_fragment(image, hunk.lines(), pos) {
            return Some(pos);
        }
    }

    None
}

fn pre_image_line_count(lines: &[Line<'_>]) -> usize {
    pre_image(lines).count()
}

fn post_image<'a, 'b>(lines: &'b [Line<'a>]) -> impl Iterator<Item = &'a str> + 'b {
    lines.iter().filter_map(|line| match line {
        Line::Context(l) | Line::Insert(l) => Some(*l),
        Line::Delete(_) => None,
    })
}

fn pre_image<'a, 'b>(lines: &'b [Line<'a>]) -> impl Iterator<Item = &'a str> + 'b {
    lines.iter().filter_map(|line| match line {
        Line::Context(l) | Line::Delete(l) => Some(*l),
        Line::Insert(_) => None,
    })
}

fn match_fragment(image: &[ImageLine], lines: &[Line<'_>], pos: usize) -> bool {
    let len = pre_image_line_count(lines);

    let image = if let Some(image) = image.get(pos..pos + len) {
        image
    } else {
        return false;
    };

    // If any of these lines have already been patched then we can't match at this position
    if image.iter().any(ImageLine::is_patched) {
        return false;
    }

    pre_image(lines).eq(image.iter().map(ImageLine::inner))
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
