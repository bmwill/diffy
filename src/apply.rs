use crate::{
    patch::{Hunk, Line, Patch},
    utils::LineIter,
};
use std::{fmt, iter};

#[derive(Debug)]
pub(crate) struct ApplyError;

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error applying patch")
    }
}

impl std::error::Error for ApplyError {}

#[allow(dead_code)]
pub(crate) fn apply(pre_image: &str, patch: &Patch<'_>) -> Result<String, ApplyError> {
    let mut image: Vec<_> = LineIter::new(pre_image).collect();

    for hunk in patch.hunks() {
        apply_hunk(&mut image, hunk)?;
    }

    Ok(image.into_iter().collect())
}

fn apply_hunk<'a>(image: &mut Vec<&'a str>, hunk: &Hunk<'a>) -> Result<(), ApplyError> {
    // Find position
    let pos = find_position(image, hunk).ok_or(ApplyError)?;

    // update image
    image.splice(
        pos..pos + pre_image_line_count(hunk.lines()),
        post_image(hunk.lines()),
    );

    Ok(())
}

fn find_position(image: &[&str], hunk: &Hunk<'_>) -> Option<usize> {
    let pos = hunk.new_range().start().saturating_sub(1);

    if match_fragment(image, hunk.lines(), pos) {
        Some(pos)
    } else {
        None
    }

    // TODO Look into finding other possible positions to apply the hunk to as described here:
    // https://neil.fraser.name/writing/patch/
    //
    // // Create an iterator that starts with 'pos' and then interleaves
    // // moving pos backward/foward by one.
    // let backward = (0..pos).rev();
    // let forward = pos + 1..image.len();
    // for pos in iter::once(pos).chain(interleave(backward, forward)) {
    //     if match_fragment(image, hunk.lines(), pos) {
    //         return Some(pos);
    //     }
    // }

    // None
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

fn match_fragment(image: &[&str], lines: &[Line<'_>], pos: usize) -> bool {
    let len = pre_image_line_count(lines);

    let image = if let Some(image) = image.get(pos..pos + len) {
        image
    } else {
        return false;
    };

    pre_image(lines).eq(image.iter().copied())
}

#[derive(Debug)]
struct Interleave<I, J> {
    a: iter::Fuse<I>,
    b: iter::Fuse<J>,
    flag: bool,
}

#[allow(dead_code)]
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
