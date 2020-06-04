use crate::{
    patch::{Hunk, HunkRange, Line, Patch},
    range::{Range, SliceLike},
};
use std::{
    cmp,
    collections::{hash_map::Entry, HashMap},
    ops,
    ops::{Index, IndexMut},
};

// A D-path is a path which starts at (0,0) that has exactly D non-diagonal edges. All D-paths
// consist of a (D - 1)-path followed by a non-diagonal edge and then a possibly empty sequence of
// diagonal edges called a snake.

/// `V` contains the endpoints of the furthest reaching `D-paths`. For each recorded endpoint
/// `(x,y)` in diagonal `k`, we only need to retain `x` because `y` can be computed from `x - k`.
/// In other words, `V` is an array of integers where `V[k]` contains the row index of the endpoint
/// of the furthest reaching path in diagonal `k`.
///
/// We can't use a traditional Vec to represent `V` since we use `k` as an index and it can take on
/// negative values. So instead `V` is represented as a light-weight wrapper around a Vec plus an
/// `offset` which is the maximum value `k` can take on in order to map negative `k`'s back to a
/// value >= 0.
#[derive(Debug, Clone)]
struct V {
    offset: isize,
    v: Vec<usize>, // Look into initializing this to -1 and storing isize
}

impl V {
    fn new(max_d: usize) -> Self {
        Self {
            offset: max_d as isize,
            v: vec![0; 2 * max_d],
        }
    }

    fn len(&self) -> usize {
        self.v.len()
    }
}

impl Index<isize> for V {
    type Output = usize;

    fn index(&self, index: isize) -> &Self::Output {
        &self.v[(index + self.offset) as usize]
    }
}

impl IndexMut<isize> for V {
    fn index_mut(&mut self, index: isize) -> &mut Self::Output {
        &mut self.v[(index + self.offset) as usize]
    }
}

/// A `Snake` is a sequence of diagonal edges in the edit graph. It is possible for a snake to have
/// a length of zero, meaning the start and end points are the same.
#[derive(Debug)]
struct Snake {
    x_start: usize,
    y_start: usize,
    x_end: usize,
    y_end: usize,
}

impl ::std::fmt::Display for Snake {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(
            f,
            "({}, {}) -> ({}, {})",
            self.x_start, self.y_start, self.x_end, self.y_end
        )
    }
}

#[derive(Debug)]
pub enum DiffRange<'a, 'b, T: ?Sized> {
    Equal(Range<'a, T>, Range<'b, T>),
    Delete(Range<'a, T>),
    Insert(Range<'b, T>),
}

impl<T: ?Sized> Copy for DiffRange<'_, '_, T> {}

impl<T: ?Sized> Clone for DiffRange<'_, '_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'tmp, 'a: 'tmp, 'b: 'tmp, T> DiffRange<'a, 'b, T>
where
    T: ?Sized + SliceLike,
{
    fn inner(&self) -> Range<'tmp, T> {
        match *self {
            DiffRange::Equal(range, _) | DiffRange::Delete(range) | DiffRange::Insert(range) => {
                range
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.inner().is_empty()
    }

    fn len(&self) -> usize {
        self.inner().len()
    }

    fn grow_up(&mut self, adjust: usize) {
        self.for_each(|range| range.grow_up(adjust));
    }

    fn grow_down(&mut self, adjust: usize) {
        self.for_each(|range| range.grow_down(adjust));
    }

    pub fn shrink_front(&mut self, adjust: usize) {
        self.for_each(|range| range.shrink_front(adjust));
    }

    pub fn shrink_back(&mut self, adjust: usize) {
        self.for_each(|range| range.shrink_back(adjust));
    }

    fn shift_up(&mut self, adjust: usize) {
        self.for_each(|range| range.shift_up(adjust));
    }

    fn shift_down(&mut self, adjust: usize) {
        self.for_each(|range| range.shift_down(adjust));
    }

    fn for_each(&mut self, f: impl Fn(&mut Range<'_, T>)) {
        match self {
            DiffRange::Equal(range1, range2) => {
                f(range1);
                f(range2);
            }
            DiffRange::Delete(range) => f(range),
            DiffRange::Insert(range) => f(range),
        }
    }
}

impl<'a, 'b> DiffRange<'a, 'b, [u8]> {
    fn to_str(&self, text1: &'a str, text2: &'b str) -> DiffRange<'a, 'b, str> {
        fn boundary_down(text: &str, pos: usize) -> usize {
            let mut adjust = 0;
            while !text.is_char_boundary(pos - adjust) {
                adjust += 1;
            }
            adjust
        }

        fn boundary_up(text: &str, pos: usize) -> usize {
            let mut adjust = 0;
            while !text.is_char_boundary(pos + adjust) {
                adjust += 1;
            }
            adjust
        }

        match self {
            DiffRange::Equal(range1, range2) => {
                debug_assert_eq!(range1.inner().as_ptr(), text1.as_ptr());
                debug_assert_eq!(range2.inner().as_ptr(), text2.as_ptr());
                let mut offset1 = range1.offset();
                let mut len1 = range1.len();
                let mut offset2 = range2.offset();
                let mut len2 = range2.len();

                let adjust = boundary_up(text1, offset1);
                offset1 += adjust;
                len1 -= adjust;
                offset2 += adjust;
                len2 -= adjust;
                let adjust = boundary_down(text1, offset1 + len1);
                len1 -= adjust;
                len2 -= adjust;

                DiffRange::Equal(
                    Range::new(text1, offset1..offset1 + len1),
                    Range::new(text2, offset2..offset2 + len2),
                )
            }
            DiffRange::Delete(range) => {
                debug_assert_eq!(range.inner().as_ptr(), text1.as_ptr());
                let mut offset = range.offset();
                let mut len = range.len();
                let adjust = boundary_down(text1, offset);
                offset -= adjust;
                len += adjust;
                let adjust = boundary_up(text1, offset + len);
                len += adjust;
                DiffRange::Delete(Range::new(text1, offset..offset + len))
            }
            DiffRange::Insert(range) => {
                debug_assert_eq!(range.inner().as_ptr(), text2.as_ptr());
                let mut offset = range.offset();
                let mut len = range.len();
                let adjust = boundary_down(text2, offset);
                offset -= adjust;
                len += adjust;
                let adjust = boundary_up(text2, offset + len);
                len += adjust;
                DiffRange::Insert(Range::new(text2, offset..offset + len))
            }
        }
    }
}

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

pub struct Myers;

impl Myers {
    fn max_d(len1: usize, len2: usize) -> usize {
        // XXX look into reducing the need to have the additional '+ 1'
        (len1 + len2 + 1) / 2 + 1
    }

    // The divide part of a divide-and-conquer strategy. A D-path has D+1 snakes some of which may
    // be empty. The divide step requires finding the ceil(D/2) + 1 or middle snake of an optimal
    // D-path. The idea for doing so is to simultaneously run the basic algorithm in both the
    // forward and reverse directions until furthest reaching forward and reverse paths starting at
    // opposing corners 'overlap'.
    fn find_middle_snake<T: PartialEq>(
        old: Range<'_, [T]>,
        new: Range<'_, [T]>,
        vf: &mut V,
        vb: &mut V,
    ) -> (isize, Snake) {
        let n = old.len();
        let m = new.len();

        // By Lemma 1 in the paper, the optimal edit script length is odd or even as `delta` is odd
        // or even.
        let delta = n as isize - m as isize;
        let odd = delta & 1 == 1;

        // The initial point at (0, -1)
        vf[1] = 0;
        // The initial point at (N, M+1)
        vb[1] = 0;

        // We only need to explore ceil(D/2) + 1
        let d_max = Self::max_d(n, m);
        assert!(vf.len() >= d_max);
        assert!(vb.len() >= d_max);

        for d in 0..d_max as isize {
            // Forward path
            for k in (-d..=d).rev().step_by(2) {
                let mut x = if k == -d || (k != d && vf[k - 1] < vf[k + 1]) {
                    vf[k + 1]
                } else {
                    vf[k - 1] + 1
                };
                let mut y = (x as isize - k) as usize;

                // The coordinate of the start of a snake
                let (x0, y0) = (x, y);
                //  While these sequences are identical, keep moving through the graph with no cost
                if let (Some(s1), Some(s2)) = (old.get(x..), new.get(y..)) {
                    let advance = s1.common_prefix_len(s2);
                    x += advance;
                    y += advance;
                }

                // This is the new best x value
                vf[k] = x;
                // Only check for connections from the forward search when N - M is odd
                // and when there is a reciprocal k line coming from the other direction.
                if odd && (k - delta).abs() <= (d - 1) {
                    // TODO optimize this so we don't have to compare against n
                    if vf[k] + vb[-(k - delta)] >= n {
                        // Return the snake
                        let snake = Snake {
                            x_start: x0,
                            y_start: y0,
                            x_end: x,
                            y_end: y,
                        };
                        // Edit distance to this snake is `2 * d - 1`
                        return (2 * d - 1, snake);
                    }
                }
            }

            // Backward path
            for k in (-d..=d).rev().step_by(2) {
                let mut x = if k == -d || (k != d && vb[k - 1] < vb[k + 1]) {
                    vb[k + 1]
                } else {
                    vb[k - 1] + 1
                };
                let mut y = (x as isize - k) as usize;

                // The coordinate of the start of a snake
                let (x0, y0) = (x, y);
                if x < n && y < m {
                    let advance = old.slice(..n - x).common_suffix_len(new.slice(..m - y));
                    x += advance;
                    y += advance;
                }

                // This is the new best x value
                vb[k] = x;

                if !odd && (k - delta).abs() <= d {
                    // TODO optimize this so we don't have to compare against n
                    if vb[k] + vf[-(k - delta)] >= n {
                        // Return the snake
                        let snake = Snake {
                            x_start: n - x,
                            y_start: m - y,
                            x_end: n - x0,
                            y_end: m - y0,
                        };
                        // Edit distance to this snake is `2 * d`
                        return (2 * d, snake);
                    }
                }
            }

            // TODO: Maybe there's an opportunity to optimize and bail early?
        }

        unreachable!("unable to find a middle snake");
    }

    fn conquer<'a, 'b, T: PartialEq>(
        mut old: Range<'a, [T]>,
        mut new: Range<'b, [T]>,
        vf: &mut V,
        vb: &mut V,
        solution: &mut Vec<DiffRange<'a, 'b, [T]>>,
    ) {
        // Check for common prefix
        let common_prefix_len = old.common_prefix_len(new);
        if common_prefix_len > 0 {
            let common_prefix = DiffRange::Equal(
                old.slice(..common_prefix_len),
                new.slice(..common_prefix_len),
            );
            solution.push(common_prefix);
        }

        old = old.slice(common_prefix_len..old.len());
        new = new.slice(common_prefix_len..new.len());

        // Check for common suffix
        let common_suffix_len = old.common_suffix_len(new);
        let common_suffix = DiffRange::Equal(
            old.slice(old.len() - common_suffix_len..),
            new.slice(new.len() - common_suffix_len..),
        );
        old = old.slice(..old.len() - common_suffix_len);
        new = new.slice(..new.len() - common_suffix_len);

        if old.is_empty() {
            // Inserts
            solution.push(DiffRange::Insert(new));
        } else if new.is_empty() {
            // Deletes
            solution.push(DiffRange::Delete(old));
        } else {
            // Divide & Conquer
            let (_shortest_edit_script_len, snake) = Self::find_middle_snake(old, new, vf, vb);

            let (old_a, old_b) = old.split_at(snake.x_start);
            let (new_a, new_b) = new.split_at(snake.y_start);

            Self::conquer(old_a, new_a, vf, vb, solution);
            Self::conquer(old_b, new_b, vf, vb, solution);
        }

        if common_suffix_len > 0 {
            solution.push(common_suffix);
        }
    }

    fn do_diff<'a, 'b, T: PartialEq>(old: &'a [T], new: &'b [T]) -> Vec<DiffRange<'a, 'b, [T]>> {
        let old_recs = Range::new(old, ..);
        let new_recs = Range::new(new, ..);

        let mut solution = Vec::new();

        // The arrays that hold the 'best possible x values' in search from:
        // `vf`: top left to bottom right
        // `vb`: bottom right to top left
        let max_d = Self::max_d(old.len(), new.len());
        let mut vf = V::new(max_d);
        let mut vb = V::new(max_d);

        Self::conquer(old_recs, new_recs, &mut vf, &mut vb, &mut solution);

        solution
    }

    pub fn diff_slice<'a, T: PartialEq>(old: &'a [T], new: &'a [T]) -> Vec<Diff<'a, [T]>> {
        let solution = Self::do_diff(old, new);

        solution.into_iter().map(Diff::from).collect()
    }

    pub fn diff<'a>(old: &'a str, new: &'a str) -> Vec<Diff<'a, str>> {
        let solution = Self::do_diff(old.as_bytes(), new.as_bytes());

        solution
            .into_iter()
            .map(|diff_range| diff_range.to_str(old, new))
            .map(Diff::from)
            .collect()
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

        let solution = Self::do_diff(&old_ids, &new_ids);

        let script = build_edit_script(&solution);
        DiffLines::new(old_lines, new_lines, script)
    }
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

#[cfg(test)]
mod tests {
    use crate::{
        diff::{Diff, Myers, V},
        range::Range,
    };

    #[test]
    fn diff_test1() {
        let a = Range::new(&b"ABCABBA"[..], ..);
        let b = Range::new(&b"CBABAC"[..], ..);
        let max_d = Myers::max_d(a.len(), b.len());
        let mut vf = V::new(max_d);
        let mut vb = V::new(max_d);
        Myers::find_middle_snake(a, b, &mut vf, &mut vb);
    }

    #[test]
    fn diff_test2() {
        let a = "ABCABBA";
        let b = "CBABAC";
        let solution = Myers::diff(a, b);
        assert_eq!(
            solution,
            vec![
                Diff::Delete("AB"),
                Diff::Equal("C"),
                Diff::Delete("A"),
                Diff::Equal("B"),
                Diff::Insert("A"),
                Diff::Equal("BA"),
                Diff::Insert("C"),
            ]
        );
    }

    #[test]
    fn diff_test3() {
        let a = "abgdef";
        let b = "gh";
        let solution = Myers::diff(a, b);
        assert_eq!(
            solution,
            vec![
                Diff::Delete("ab"),
                Diff::Equal("g"),
                Diff::Insert("h"),
                Diff::Delete("def"),
            ]
        );
    }

    #[test]
    fn diff_test4() {
        let a = "bat";
        let b = "map";
        let solution = Myers::diff_slice(a.as_bytes(), b.as_bytes());
        let expected: Vec<Diff<[u8]>> = vec![
            Diff::Insert(b"m"),
            Diff::Delete(b"b"),
            Diff::Equal(b"a"),
            Diff::Insert(b"p"),
            Diff::Delete(b"t"),
        ];
        assert_eq!(solution, expected);

        let solution = Myers::diff(a, b);
        assert_eq!(
            solution,
            vec![
                Diff::Insert("m"),
                Diff::Delete("b"),
                Diff::Equal("a"),
                Diff::Insert("p"),
                Diff::Delete("t"),
            ]
        );
    }

    #[test]
    fn diff_test5() {
        let a = "abc";
        let b = "def";
        let solution = Myers::diff(a, b);
        assert_eq!(solution, vec![Diff::Insert("def"), Diff::Delete("abc"),]);
    }

    #[test]
    fn diff_str() {
        let a = "A\nB\nC\nA\nB\nB\nA";
        let b = "C\nB\nA\nB\nA\nC";
        let diff = Myers::diff_lines(a, b);
        let expected = "\
--- a
+++ b
@@ -1,7 +1,6 @@
-A
-B
 C
-A
 B
+A
 B
 A
+C
";

        assert_eq!(diff.to_patch(3).to_string(), expected);
    }

    #[test]
    fn sample() {
        let lao = "\
The Way that can be told of is not the eternal Way;
The name that can be named is not the eternal name.
The Nameless is the origin of Heaven and Earth;
The Named is the mother of all things.
Therefore let there always be non-being,
  so we may see their subtlety,
And let there always be being,
  so we may see their outcome.
The two are the same,
But after they are produced,
  they have different names.
";

        let tzu = "\
The Nameless is the origin of Heaven and Earth;
The named is the mother of all things.

Therefore let there always be non-being,
  so we may see their subtlety,
And let there always be being,
  so we may see their outcome.
The two are the same,
But after they are produced,
  they have different names.
They both may be called deep and profound.
Deeper and more profound,
The door of all subtleties!
";

        let expected = "\
--- a
+++ b
@@ -1,7 +1,6 @@
-The Way that can be told of is not the eternal Way;
-The name that can be named is not the eternal name.
 The Nameless is the origin of Heaven and Earth;
-The Named is the mother of all things.
+The named is the mother of all things.
+
 Therefore let there always be non-being,
   so we may see their subtlety,
 And let there always be being,
@@ -9,3 +8,6 @@
 The two are the same,
 But after they are produced,
   they have different names.
+They both may be called deep and profound.
+Deeper and more profound,
+The door of all subtleties!
";

        let diff = Myers::diff_lines(lao, tzu);
        assert_eq!(diff.to_patch(3).to_string(), expected);

        let expected = "\
--- a
+++ b
@@ -1,2 +0,0 @@
-The Way that can be told of is not the eternal Way;
-The name that can be named is not the eternal name.
@@ -4 +2,2 @@
-The Named is the mother of all things.
+The named is the mother of all things.
+
@@ -11,0 +11,3 @@
+They both may be called deep and profound.
+Deeper and more profound,
+The door of all subtleties!
";
        assert_eq!(diff.to_patch(0).to_string(), expected);

        let expected = "\
--- a
+++ b
@@ -1,5 +1,4 @@
-The Way that can be told of is not the eternal Way;
-The name that can be named is not the eternal name.
 The Nameless is the origin of Heaven and Earth;
-The Named is the mother of all things.
+The named is the mother of all things.
+
 Therefore let there always be non-being,
@@ -11 +10,4 @@
   they have different names.
+They both may be called deep and profound.
+Deeper and more profound,
+The door of all subtleties!
";
        assert_eq!(diff.to_patch(1).to_string(), expected);
    }

    // XXX Fix this test once we implement a cleanup pass to remove the empty Equality
    // XXX Fix this test once we have a cleanup pass to reorder Deletions before Insertions
    #[test]
    fn test_unicode() {
        // Unicode snowman and unicode comet have the same first two bytes. A
        // byte-based diff would produce a 2-byte Equal followed by 1-byte Delete
        // and Insert.
        let snowman = "\u{2603}";
        let comet = "\u{2604}";
        assert_eq!(snowman.as_bytes()[..2], comet.as_bytes()[..2]);

        let d = Myers::diff(snowman, comet);
        assert_eq!(
            d,
            vec![Diff::Equal(""), Diff::Insert(comet), Diff::Delete(snowman)]
        );
    }
}
