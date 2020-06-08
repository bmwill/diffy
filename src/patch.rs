use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch<'a> {
    old: Option<&'a str>,
    new: Option<&'a str>,
    hunks: Vec<Hunk<'a>>,
}

impl<'a> Patch<'a> {
    pub(crate) fn new(old: Option<&'a str>, new: Option<&'a str>, hunks: Vec<Hunk<'a>>) -> Self {
        Self { old, new, hunks }
    }
}

impl fmt::Display for Patch<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "--- a")?;
        writeln!(f, "+++ b")?;

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

impl<'a> Hunk<'a> {
    pub(crate) fn new(old_range: HunkRange, new_range: HunkRange, lines: Vec<Line<'a>>) -> Self {
        let mut old_count = 0;
        let mut new_count = 0;
        for line in &lines {
            match line {
                Line::Context(_) => {
                    old_count += 1;
                    new_count += 1;
                }
                Line::Delete(_) => old_count += 1,
                Line::Insert(_) => new_count += 1,
            }
        }

        assert_eq!(old_range.len, old_count);
        assert_eq!(new_range.len, new_count);

        Self {
            old_range,
            new_range,
            function_context: None,
            lines,
        }
    }
}

impl fmt::Display for Hunk<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "@@ -{} +{} @@", self.old_range, self.new_range)?;
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
