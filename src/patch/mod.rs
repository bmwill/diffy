use std::{fmt, ops};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch<'a> {
    original: &'a str,
    modified: &'a str,
    hunks: Vec<Hunk<'a>>,
}

impl<'a> Patch<'a> {
    pub(crate) fn new(original: &'a str, modified: &'a str, hunks: Vec<Hunk<'a>>) -> Self {
        Self {
            original,
            modified,
            hunks,
        }
    }

    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn modified(&self) -> &str {
        &self.modified
    }

    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }
}

impl fmt::Display for Patch<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "--- {}", self.original)?;
        writeln!(f, "+++ {}", self.modified)?;

        for hunk in &self.hunks {
            write!(f, "{}", hunk)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk<'a> {
    old_range: HunkRange,
    new_range: HunkRange,

    function_context: Option<&'a str>,

    lines: Vec<Line<'a>>,
}

fn hunk_lines_count(lines: &[Line<'_>]) -> (usize, usize) {
    lines.into_iter().fold((0, 0), |count, line| match line {
        Line::Context(_) => (count.0 + 1, count.1 + 1),
        Line::Delete(_) => (count.0 + 1, count.1),
        Line::Insert(_) => (count.0, count.1 + 1),
    })
}

impl<'a> Hunk<'a> {
    pub(crate) fn new(
        old_range: HunkRange,
        new_range: HunkRange,
        function_context: Option<&'a str>,
        lines: Vec<Line<'a>>,
    ) -> Self {
        let (old_count, new_count) = hunk_lines_count(&lines);

        assert_eq!(old_range.len, old_count);
        assert_eq!(new_range.len, new_count);

        Self {
            old_range,
            new_range,
            function_context,
            lines,
        }
    }
}

impl fmt::Display for Hunk<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@@ -{} +{} @@", self.old_range, self.new_range)?;
        if let Some(ctx) = self.function_context {
            write!(f, " {}", ctx)?;
        }
        writeln!(f)?;
        for line in &self.lines {
            write!(f, "{}", line)?;
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HunkRange {
    /// The starting line number of a hunk
    start: usize,
    /// The hunk size (number of lines)
    len: usize,
}

impl HunkRange {
    pub(crate) fn new(start: usize, len: usize) -> Self {
        Self { start, len }
    }

    pub fn range(&self) -> ops::Range<usize> {
        self.start..self.start + self.len
    }
}

impl fmt::Display for HunkRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.start)?;
        if self.len != 1 {
            write!(f, ",{}", self.len)?;
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Line<'a> {
    /// A line providing context in the diff which is present in both the old and new file
    Context(&'a str),
    /// A line deleted from the old file
    Delete(&'a str),
    /// A line inserted to the new file
    Insert(&'a str),
}

impl fmt::Display for Line<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (sign, line) = match self {
            Line::Context(line) => (' ', line),
            Line::Delete(line) => ('-', line),
            Line::Insert(line) => ('+', line),
        };

        write!(f, "{}{}", sign, line)?;

        if !line.ends_with('\n') {
            writeln!(f, "\n\\ No newline at end of file")?;
        }

        Ok(())
    }
}

#[allow(dead_code)]
pub(crate) fn apply(pre_image: &str, patch: &Patch<'_>) -> String {
    let pre_image: Vec<_> = crate::diff::LineIter(pre_image).collect();
    let mut image = pre_image.clone();

    for hunk in patch.hunks() {
        apply_hunk(&pre_image, &mut image, hunk);
    }

    image.into_iter().collect()
}

fn apply_hunk<'a>(pre_image: &[&'a str], image: &mut Vec<&'a str>, hunk: &Hunk<'a>) {
    let mut pos1 = hunk.old_range.start.saturating_sub(1);
    let mut pos2 = hunk.new_range.start.saturating_sub(1);

    for line in &hunk.lines {
        match line {
            Line::Context(line) => {
                if let (Some(old), Some(new)) = (pre_image.get(pos1), image.get(pos2)) {
                    if !(line == old && line == new) {
                        panic!("Does not apply");
                    }
                } else {
                    panic!("ERR");
                }
                pos1 += 1;
                pos2 += 1;
            }
            Line::Delete(line) => {
                if line != &pre_image[pos1] {
                    panic!("Does not apply");
                }

                if line != &image[pos2] {
                    panic!("Does not apply");
                }

                image.remove(pos2);
                pos1 += 1;
            }
            Line::Insert(line) => {
                image.insert(pos2, line);
                pos2 += 1;
            }
        }
    }
}
