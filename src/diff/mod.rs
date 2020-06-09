use crate::{
    patch::{Hunk, HunkRange, Line, Patch},
    range::{DiffRange, SliceLike},
};
use std::{
    cmp,
    collections::{hash_map::Entry, HashMap},
    ops,
};

mod cleanup;
mod myers;

#[cfg(test)]
mod tests;

#[derive(Debug, PartialEq, Eq)]
pub enum Diff<'a, T: ?Sized> {
    Equal(&'a T),
    Delete(&'a T),
    Insert(&'a T),
}

impl<T: ?Sized> Copy for Diff<'_, T> {}

impl<T: ?Sized> Clone for Diff<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> From<DiffRange<'a, 'a, T>> for Diff<'a, T>
where
    T: ?Sized + SliceLike,
{
    fn from(diff: DiffRange<'a, 'a, T>) -> Self {
        match diff {
            DiffRange::Equal(range, _) => Diff::Equal(range.as_slice()),
            DiffRange::Delete(range) => Diff::Delete(range.as_slice()),
            DiffRange::Insert(range) => Diff::Insert(range.as_slice()),
        }
    }
}

#[derive(Debug)]
pub struct DiffOptions {
    compact: bool,
    context_len: usize,
}

impl DiffOptions {
    pub fn new() -> Self {
        Self {
            compact: true,
            context_len: 3,
        }
    }

    pub fn set_context_len(&mut self, context_len: usize) -> &mut Self {
        self.context_len = context_len;
        self
    }

    pub fn set_compact(&mut self, compact: bool) -> &mut Self {
        self.compact = compact;
        self
    }

    pub fn diff<'a>(&self, original: &'a str, modified: &'a str) -> Vec<Diff<'a, str>> {
        let solution = myers::diff(original.as_bytes(), modified.as_bytes());

        let mut solution = solution
            .into_iter()
            .map(|diff_range| diff_range.to_str(original, modified))
            .collect();

        if self.compact {
            cleanup::compact(&mut solution);
        }

        solution.into_iter().map(Diff::from).collect()
    }

    pub fn create_patch<'a>(&self, original: &'a str, modified: &'a str) -> Patch<'a> {
        let mut classifier = Classifier::default();
        let (old_lines, old_ids) = classifier.classify_lines(original);
        let (new_lines, new_ids) = classifier.classify_lines(modified);

        let mut solution = myers::diff(&old_ids, &new_ids);

        if self.compact {
            cleanup::compact(&mut solution);
        }

        to_patch(&old_lines, &new_lines, &solution, self.context_len)
    }

    // TODO determine if this should be exposed in the public API
    #[allow(dead_code)]
    fn diff_slice<'a, T: PartialEq>(&self, old: &'a [T], new: &'a [T]) -> Vec<Diff<'a, [T]>> {
        let mut solution = myers::diff(old, new);

        if self.compact {
            cleanup::compact(&mut solution);
        }

        solution.into_iter().map(Diff::from).collect()
    }
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self::new()
    }
}

pub fn diff<'a>(original: &'a str, modified: &'a str) -> Vec<Diff<'a, str>> {
    DiffOptions::default().diff(original, modified)
}

pub fn create_patch<'a>(original: &'a str, modified: &'a str) -> Patch<'a> {
    DiffOptions::default().create_patch(original, modified)
}

#[derive(Default)]
struct Classifier<'a> {
    next_id: u64,
    unique_ids: HashMap<&'a str, u64>,
}

impl<'a> Classifier<'a> {
    fn classify(&mut self, record: &'a str) -> u64 {
        match self.unique_ids.entry(record) {
            Entry::Occupied(o) => *o.get(),
            Entry::Vacant(v) => {
                let id = self.next_id;
                self.next_id += 1;
                *v.insert(id)
            }
        }
    }

    fn classify_lines(&mut self, text: &'a str) -> (Vec<&'a str>, Vec<u64>) {
        LineIter(text)
            .map(|line| (line, self.classify(&line)))
            .unzip()
    }
}

/// Iterator over the lines of a string, including the `\n` character.
pub(crate) struct LineIter<'a>(pub(crate) &'a str);

impl<'a> Iterator for LineIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }

        let end = if let Some(idx) = self.0.find('\n') {
            idx + 1
        } else {
            self.0.len()
        };

        let (line, remaining) = self.0.split_at(end);
        self.0 = remaining;
        Some(line)
    }
}

fn to_patch<'a>(
    lines1: &[&'a str],
    lines2: &[&'a str],
    solution: &[DiffRange<[u64]>],
    context_len: usize,
) -> Patch<'a> {
    let edit_script = build_edit_script(solution);

    let mut hunks = Vec::new();

    let mut idx = 0;
    while let Some(mut script) = edit_script.get(idx) {
        let start1 = script.old.start.saturating_sub(context_len);
        let start2 = script.new.start.saturating_sub(context_len);

        let (mut end1, mut end2) = calc_end(
            context_len,
            lines1.len(),
            lines2.len(),
            script.old.end,
            script.new.end,
        );

        let mut lines = Vec::new();

        // Pre-context
        for line in lines2.get(start2..script.new.start).into_iter().flatten() {
            lines.push(Line::Context(line));
        }

        loop {
            // Delete lines from text1
            for line in lines1.get(script.old.clone()).into_iter().flatten() {
                lines.push(Line::Delete(line));
            }

            // Insert lines from text2
            for line in lines2.get(script.new.clone()).into_iter().flatten() {
                lines.push(Line::Insert(line));
            }

            if let Some(s) = edit_script.get(idx + 1) {
                // Check to see if we can merge the hunks
                let start1_next =
                    cmp::min(s.old.start, lines1.len() - 1).saturating_sub(context_len);
                if start1_next < end1 {
                    // Context lines between hunks
                    for (_i1, i2) in (script.old.end..s.old.start).zip(script.new.end..s.new.start)
                    {
                        if let Some(line) = lines2.get(i2) {
                            lines.push(Line::Context(line));
                        }
                    }

                    // Calc the new end
                    let (e1, e2) = calc_end(
                        context_len,
                        lines1.len(),
                        lines2.len(),
                        s.old.end,
                        s.new.end,
                    );

                    end1 = e1;
                    end2 = e2;
                    script = s;
                    idx += 1;
                    continue;
                }
            }

            break;
        }

        // Post-context
        for line in lines2.get(script.new.end..end2).into_iter().flatten() {
            lines.push(Line::Context(line));
        }

        let len1 = end1 - start1;
        let old_range = HunkRange::new(if len1 > 0 { start1 + 1 } else { start1 }, len1);

        let len2 = end2 - start2;
        let new_range = HunkRange::new(if len2 > 0 { start2 + 1 } else { start2 }, len2);

        hunks.push(Hunk::new(old_range, new_range, lines));
        idx += 1;
    }

    Patch::new("original", "modified", hunks)
}

fn calc_end(
    context_len: usize,
    text1_len: usize,
    text2_len: usize,
    script1_end: usize,
    script2_end: usize,
) -> (usize, usize) {
    let post_context_len = cmp::min(
        context_len,
        cmp::min(
            text1_len.saturating_sub(script1_end),
            text2_len.saturating_sub(script2_end),
        ),
    );

    let end1 = script1_end + post_context_len;
    let end2 = script2_end + post_context_len;

    (end1, end2)
}

#[derive(Debug)]
struct EditRange {
    old: ops::Range<usize>,
    new: ops::Range<usize>,
}

impl EditRange {
    fn new(old: ops::Range<usize>, new: ops::Range<usize>) -> Self {
        Self { old, new }
    }
}

fn build_edit_script<T>(solution: &[DiffRange<[T]>]) -> Vec<EditRange> {
    let mut idx_a = 0;
    let mut idx_b = 0;

    let mut edit_script: Vec<EditRange> = Vec::new();
    let mut script = None;

    for diff in solution {
        match diff {
            DiffRange::Equal(range1, range2) => {
                idx_a += range1.len();
                idx_b += range2.len();
                if let Some(script) = script.take() {
                    edit_script.push(script);
                }
            }
            DiffRange::Delete(range) => {
                match &mut script {
                    Some(s) => s.old.end += range.len(),
                    None => {
                        script = Some(EditRange::new(idx_a..idx_a + range.len(), idx_b..idx_b));
                    }
                }
                idx_a += range.len();
            }
            DiffRange::Insert(range) => {
                match &mut script {
                    Some(s) => s.new.end += range.len(),
                    None => {
                        script = Some(EditRange::new(idx_a..idx_a, idx_b..idx_b + range.len()));
                    }
                }
                idx_b += range.len();
            }
        }
    }

    if let Some(script) = script.take() {
        edit_script.push(script);
    }

    edit_script
}
