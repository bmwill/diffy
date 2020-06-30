use super::{Hunk, Line, Patch, NO_NEWLINE_AT_EOF};
use ansi_term::{Color, Style};
use std::fmt::{Display, Formatter, Result};

/// Struct used to adjust the formatting of a `Patch`
#[derive(Debug)]
pub struct PatchFormatter {
    with_color: bool,

    context: Style,
    delete: Style,
    insert: Style,
    hunk_header: Style,
    patch_header: Style,
    function_context: Style,
}

impl PatchFormatter {
    /// Construct a new formatter
    pub fn new() -> Self {
        Self {
            with_color: false,

            context: Style::new(),
            delete: Color::Red.normal(),
            insert: Color::Green.normal(),
            hunk_header: Color::Cyan.normal(),
            patch_header: Style::new().bold(),
            function_context: Style::new(),
        }
    }

    /// Enable formatting a patch with color
    pub fn with_color(mut self) -> Self {
        self.with_color = true;
        self
    }

    /// Returns a `Display` impl which can be used to print a Patch
    pub fn fmt_patch<'a>(&'a self, patch: &'a Patch<'a>) -> impl Display + 'a {
        PatchDisplay { f: self, patch }
    }

    fn fmt_hunk<'a>(&'a self, hunk: &'a Hunk<'a>) -> impl Display + 'a {
        HunkDisplay { f: self, hunk }
    }

    fn fmt_line<'a>(&'a self, line: &'a Line<'a>) -> impl Display + 'a {
        LineDisplay { f: self, line }
    }
}

impl Default for PatchFormatter {
    fn default() -> Self {
        Self::new()
    }
}

struct PatchDisplay<'a> {
    f: &'a PatchFormatter,
    patch: &'a Patch<'a>,
}

impl Display for PatchDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.f.with_color {
            write!(f, "{}", self.f.patch_header.prefix())?;
        }
        writeln!(f, "--- {}", self.patch.original)?;
        writeln!(f, "+++ {}", self.patch.modified)?;
        if self.f.with_color {
            write!(f, "{}", self.f.patch_header.suffix())?;
        }

        for hunk in &self.patch.hunks {
            write!(f, "{}", self.f.fmt_hunk(hunk))?;
        }

        Ok(())
    }
}

struct HunkDisplay<'a> {
    f: &'a PatchFormatter,
    hunk: &'a Hunk<'a>,
}

impl Display for HunkDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.f.with_color {
            write!(f, "{}", self.f.hunk_header.prefix())?;
        }
        write!(f, "@@ -{} +{} @@", self.hunk.old_range, self.hunk.new_range)?;
        if self.f.with_color {
            write!(f, "{}", self.f.hunk_header.suffix())?;
        }

        if let Some(ctx) = self.hunk.function_context {
            write!(f, " ")?;
            if self.f.with_color {
                write!(f, "{}", self.f.function_context.prefix())?;
            }
            write!(f, " {}", ctx)?;
            if self.f.with_color {
                write!(f, "{}", self.f.function_context.suffix())?;
            }
        }
        writeln!(f)?;

        for line in &self.hunk.lines {
            write!(f, "{}", self.f.fmt_line(line))?;
        }

        Ok(())
    }
}

struct LineDisplay<'a> {
    f: &'a PatchFormatter,
    line: &'a Line<'a>,
}

impl Display for LineDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let (sign, line, style) = match self.line {
            Line::Context(line) => (' ', line, self.f.context),
            Line::Delete(line) => ('-', line, self.f.delete),
            Line::Insert(line) => ('+', line, self.f.insert),
        };

        if self.f.with_color {
            write!(f, "{}", style.prefix())?;
        }

        if sign == ' ' && *line == "\n" {
            write!(f, "{}", line)?;
        } else {
            write!(f, "{}{}", sign, line)?;
        }

        if self.f.with_color {
            write!(f, "{}", style.suffix())?;
        }

        if !line.ends_with('\n') {
            writeln!(f)?;
            writeln!(f, "{}", NO_NEWLINE_AT_EOF)?;
        }

        Ok(())
    }
}
