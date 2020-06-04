use crate::{
    patch::{Hunk, HunkRange, Line, Patch},
    range::{DiffRange, SliceLike},
};
use std::{
    cmp,
    collections::{hash_map::Entry, HashMap},
    ops,
};

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

pub fn diff_slice<'a, T: PartialEq>(old: &'a [T], new: &'a [T]) -> Vec<Diff<'a, [T]>> {
    let mut solution = myers::diff(old, new);
    compact(&mut solution);

    solution.into_iter().map(Diff::from).collect()
}

pub fn diff<'a>(old: &'a str, new: &'a str) -> Vec<Diff<'a, str>> {
    let solution = myers::diff(old.as_bytes(), new.as_bytes());

    let mut solution = solution
        .into_iter()
        .map(|diff_range| diff_range.to_str(old, new))
        .collect();

    compact(&mut solution);

    solution.into_iter().map(Diff::from).collect()
}

pub fn diff_lines<'a>(old: &'a str, new: &'a str) -> DiffLines<'a> {
    let mut classifier = Classifier::default();
    let (old_lines, old_ids): (Vec<&str>, Vec<u64>) = old
        .lines()
        .map(|line| (line, classifier.classify(&line)))
        .unzip();
    let (new_lines, new_ids): (Vec<&str>, Vec<u64>) = new
        .lines()
        .map(|line| (line, classifier.classify(&line)))
        .unzip();

    let mut solution = myers::diff(&old_ids, &new_ids);
    compact(&mut solution);

    let script = build_edit_script(&solution);
    DiffLines::new(old_lines, new_lines, script)
}

// Walks through all edits and shifts them up and then down, trying to see if they run into similar
// edits which can be merged
fn compact<'a, 'b, T: ?Sized + SliceLike>(diffs: &mut Vec<DiffRange<'a, 'b, T>>) {
    // First attempt to compact all Deletions
    let mut pointer = 0;
    while let Some(&diff) = diffs.get(pointer) {
        if let DiffRange::Delete(_) = diff {
            pointer = shift_diff_up(diffs, pointer);
            pointer = shift_diff_down(diffs, pointer);
        }
        pointer += 1;
    }

    // TODO maybe able to merge these and do them in the same pass?
    // Then attempt to compact all Deletions
    let mut pointer = 0;
    while let Some(&diff) = diffs.get(pointer) {
        if let DiffRange::Insert(_) = diff {
            pointer = shift_diff_up(diffs, pointer);
            pointer = shift_diff_down(diffs, pointer);
        }
        pointer += 1;
    }
}

// Attempts to shift the Insertion or Deletion at location `pointer` as far upwards as possible.
fn shift_diff_up<'a, 'b, T: ?Sized + SliceLike>(
    diffs: &mut Vec<DiffRange<'a, 'b, T>>,
    mut pointer: usize,
) -> usize {
    while let Some(&prev_diff) = pointer.checked_sub(1).and_then(|idx| diffs.get(idx)) {
        match (diffs[pointer], prev_diff) {
            //
            // Shift Inserts Upwards
            //
            (DiffRange::Insert(this_diff), DiffRange::Equal(prev_diff1, _)) => {
                // check common suffix for the amount we can shift
                let suffix_len = this_diff.common_suffix_len(prev_diff1);
                if suffix_len != 0 {
                    if let Some(DiffRange::Equal(..)) = diffs.get(pointer + 1) {
                        diffs[pointer + 1].grow_up(suffix_len);
                    } else {
                        diffs.insert(
                            pointer + 1,
                            DiffRange::Equal(
                                prev_diff1.slice(prev_diff1.len() - suffix_len..),
                                this_diff.slice(this_diff.len() - suffix_len..),
                            ),
                        );
                    }
                    diffs[pointer].shift_up(suffix_len);
                    diffs[pointer - 1].shrink_back(suffix_len);

                    if diffs[pointer - 1].is_empty() {
                        diffs.remove(pointer - 1);
                        pointer -= 1;
                    }
                } else if diffs[pointer - 1].is_empty() {
                    diffs.remove(pointer - 1);
                    pointer -= 1;
                } else {
                    // We can't shift upwards anymore
                    break;
                }
            }

            //
            // Shift Deletions Upwards
            //
            (DiffRange::Delete(this_diff), DiffRange::Equal(_, prev_diff2)) => {
                // check common suffix for the amount we can shift
                let suffix_len = this_diff.common_suffix_len(prev_diff2);
                if suffix_len != 0 {
                    if let Some(DiffRange::Equal(..)) = diffs.get(pointer + 1) {
                        diffs[pointer + 1].grow_up(suffix_len);
                    } else {
                        diffs.insert(
                            pointer + 1,
                            DiffRange::Equal(
                                this_diff.slice(this_diff.len() - suffix_len..),
                                prev_diff2.slice(prev_diff2.len() - suffix_len..),
                            ),
                        );
                    }
                    diffs[pointer].shift_up(suffix_len);
                    diffs[pointer - 1].shrink_back(suffix_len);

                    if diffs[pointer - 1].is_empty() {
                        diffs.remove(pointer - 1);
                        pointer -= 1;
                    }
                } else if diffs[pointer - 1].is_empty() {
                    diffs.remove(pointer - 1);
                    pointer -= 1;
                } else {
                    // We can't shift upwards anymore
                    break;
                }
            }

            //
            // Swap the Delete and Insert
            //
            (DiffRange::Insert(_), DiffRange::Delete(_))
            | (DiffRange::Delete(_), DiffRange::Insert(_)) => {
                diffs.swap(pointer - 1, pointer);
                pointer -= 1;
            }

            //
            // Merge the two ranges
            //
            (this_diff @ DiffRange::Insert(_), DiffRange::Insert(_))
            | (this_diff @ DiffRange::Delete(_), DiffRange::Delete(_)) => {
                diffs[pointer - 1].grow_down(this_diff.len());
                diffs.remove(pointer);
                pointer -= 1;
            }

            _ => panic!("range to shift must be either Insert or Delete"),
        }
    }

    pointer
}

// Attempts to shift the Insertion or Deletion at location `pointer` as far downwards as possible.
fn shift_diff_down<'a, 'b, T: ?Sized + SliceLike>(
    diffs: &mut Vec<DiffRange<'a, 'b, T>>,
    mut pointer: usize,
) -> usize {
    while let Some(&next_diff) = pointer.checked_add(1).and_then(|idx| diffs.get(idx)) {
        match (diffs[pointer], next_diff) {
            //
            // Shift Insert Downward
            //
            (DiffRange::Insert(this_diff), DiffRange::Equal(next_diff1, _)) => {
                // check common prefix for the amoutn we can shift
                let prefix_len = this_diff.common_prefix_len(next_diff1);
                if prefix_len != 0 {
                    if let Some(DiffRange::Equal(..)) =
                        pointer.checked_sub(1).and_then(|idx| diffs.get(idx))
                    {
                        diffs[pointer - 1].grow_down(prefix_len);
                    } else {
                        diffs.insert(
                            pointer,
                            DiffRange::Equal(
                                next_diff1.slice(..prefix_len),
                                this_diff.slice(..prefix_len),
                            ),
                        );
                        pointer += 1;
                    }

                    diffs[pointer].shift_down(prefix_len);
                    diffs[pointer + 1].shrink_front(prefix_len);

                    if diffs[pointer + 1].is_empty() {
                        diffs.remove(pointer + 1);
                    }
                } else if diffs[pointer + 1].is_empty() {
                    diffs.remove(pointer + 1);
                } else {
                    // We can't shift downwards anymore
                    break;
                }
            }

            //
            // Shift Deletion Downward
            //
            (DiffRange::Delete(this_diff), DiffRange::Equal(_, next_diff2)) => {
                // check common prefix for the amoutn we can shift
                let prefix_len = this_diff.common_prefix_len(next_diff2);
                if prefix_len != 0 {
                    if let Some(DiffRange::Equal(..)) =
                        pointer.checked_sub(1).and_then(|idx| diffs.get(idx))
                    {
                        diffs[pointer - 1].grow_down(prefix_len);
                    } else {
                        diffs.insert(
                            pointer,
                            DiffRange::Equal(
                                this_diff.slice(..prefix_len),
                                next_diff2.slice(..prefix_len),
                            ),
                        );
                        pointer += 1;
                    }

                    diffs[pointer].shift_down(prefix_len);
                    diffs[pointer + 1].shrink_front(prefix_len);

                    if diffs[pointer + 1].is_empty() {
                        diffs.remove(pointer + 1);
                    }
                } else if diffs[pointer + 1].is_empty() {
                    diffs.remove(pointer + 1);
                } else {
                    // We can't shift downwards anymore
                    break;
                }
            }

            //
            // Swap the Delete and Insert
            //
            (DiffRange::Insert(_), DiffRange::Delete(_))
            | (DiffRange::Delete(_), DiffRange::Insert(_)) => {
                diffs.swap(pointer, pointer + 1);
                pointer += 1;
            }

            //
            // Merge the two ranges
            //
            (DiffRange::Insert(_), next_diff @ DiffRange::Insert(_))
            | (DiffRange::Delete(_), next_diff @ DiffRange::Delete(_)) => {
                diffs[pointer].grow_down(next_diff.len());
                diffs.remove(pointer + 1);
            }

            _ => panic!("range to shift must be either Insert or Delete"),
        }
    }

    pointer
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
}

#[derive(Debug)]
pub struct DiffLines<'a> {
    a_text: Vec<&'a str>,
    b_text: Vec<&'a str>,
    edit_script: Vec<EditRange>,
}

impl<'a> DiffLines<'a> {
    fn new(a_text: Vec<&'a str>, b_text: Vec<&'a str>, edit_script: Vec<EditRange>) -> Self {
        Self {
            a_text,
            b_text,
            edit_script,
        }
    }

    pub fn to_patch(&self, context_len: usize) -> Patch {
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

        let mut hunks = Vec::new();

        let mut idx = 0;
        while let Some(mut script) = self.edit_script.get(idx) {
            let start1 = script.old.start.saturating_sub(context_len);
            let start2 = script.new.start.saturating_sub(context_len);

            let (mut end1, mut end2) = calc_end(
                context_len,
                self.a_text.len(),
                self.b_text.len(),
                script.old.end,
                script.new.end,
            );

            let mut lines = Vec::new();

            // Pre-context
            for line in self
                .b_text
                .get(start2..script.new.start)
                .into_iter()
                .flatten()
            {
                lines.push(Line::Context(line));
            }

            loop {
                // Delete lines from text1
                for line in self
                    .a_text
                    .get(script.old.start..script.old.end)
                    .into_iter()
                    .flatten()
                {
                    lines.push(Line::Delete(line));
                }

                // Insert lines from text2
                for line in self
                    .b_text
                    .get(script.new.start..script.new.end)
                    .into_iter()
                    .flatten()
                {
                    lines.push(Line::Insert(line));
                }

                if let Some(s) = self.edit_script.get(idx + 1) {
                    // Check to see if we can merge the hunks
                    let start1_next =
                        cmp::min(s.old.start, self.a_text.len() - 1).saturating_sub(context_len);
                    if start1_next < end1 {
                        // Context lines between hunks
                        for (_i1, i2) in
                            (script.old.end..s.old.start).zip(script.new.end..s.new.start)
                        {
                            if let Some(line) = self.b_text.get(i2) {
                                lines.push(Line::Context(line));
                            }
                        }

                        // Calc the new end
                        let (e1, e2) = calc_end(
                            context_len,
                            self.a_text.len(),
                            self.b_text.len(),
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
            for line in self.b_text.get(script.new.end..end2).into_iter().flatten() {
                lines.push(Line::Context(line));
            }

            let len1 = end1 - start1;
            let old_range = HunkRange::new(if len1 > 0 { start1 + 1 } else { start1 }, len1);

            let len2 = end2 - start2;
            let new_range = HunkRange::new(if len2 > 0 { start2 + 1 } else { start2 }, len2);

            hunks.push(Hunk::new(old_range, new_range, lines));
            idx += 1;
        }

        Patch::new(None, None, hunks)
    }
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
