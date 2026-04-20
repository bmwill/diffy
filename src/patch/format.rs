use alloc::borrow::ToOwned;
use core::fmt::Display;
use core::fmt::Formatter;
use core::fmt::Result;

#[cfg(feature = "std")]
use std::io;

#[cfg(feature = "color")]
use super::style;
use super::Hunk;
use super::Line;
use super::Patch;
use super::NO_NEWLINE_AT_EOF;

/// Struct used to adjust the formatting of a `Patch`
#[derive(Debug)]
pub struct PatchFormatter {
    #[cfg(feature = "color")]
    with_color: bool,
    with_missing_newline_message: bool,
    suppress_blank_empty: bool,
}

impl PatchFormatter {
    /// Construct a new formatter
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "color")]
            with_color: false,
            with_missing_newline_message: true,

            // TODO the default in git-diff and GNU diff is to have this set to false, on the next
            // semver breaking release we should contemplate switching this to be false by default
            suppress_blank_empty: true,
        }
    }

    /// Enable formatting a patch with color
    #[cfg(feature = "color")]
    #[cfg_attr(docsrs, doc(cfg(feature = "color")))]
    pub fn with_color(mut self) -> Self {
        self.with_color = true;
        self
    }

    /// Sets whether to format a patch with a "No newline at end of file" message.
    ///
    /// Default is `true`.
    ///
    /// Note: If this is disabled by setting to `false`, formatted patches will no longer contain
    /// sufficient information to determine if a file ended with a newline character (`\n`) or not
    /// and the patch will be formatted as if both the original and modified files ended with a
    /// newline character (`\n`).
    pub fn missing_newline_message(mut self, enable: bool) -> Self {
        self.with_missing_newline_message = enable;
        self
    }

    /// Sets whether to suppress printing of a space before empty lines.
    ///
    /// Defaults to `true`.
    ///
    /// For more information you can refer to the [Omitting trailing blanks] manual page of GNU
    /// diff or the [diff.suppressBlankEmpty] config for `git-diff`.
    ///
    /// [Omitting trailing blanks]: https://www.gnu.org/software/diffutils/manual/html_node/Trailing-Blanks.html
    /// [diff.suppressBlankEmpty]: https://git-scm.com/docs/git-diff#Documentation/git-diff.txt-codediffsuppressBlankEmptycode
    pub fn suppress_blank_empty(mut self, enable: bool) -> Self {
        self.suppress_blank_empty = enable;
        self
    }

    /// Returns a `Display` impl which can be used to print a Patch
    pub fn fmt_patch<'a>(&'a self, patch: &'a Patch<'a, str>) -> impl Display + 'a {
        PatchDisplay { f: self, patch }
    }

    /// Writes a formatted patch into a writer.
    ///
    /// This is the byte-oriented equivalent of [`fmt_patch`](Self::fmt_patch)
    /// for callers that want to stream the formatted patch into an `io::Write`
    /// sink instead of going through `Display`.
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub fn write_patch_into<T: ToOwned + AsRef<[u8]> + ?Sized, W: io::Write>(
        &self,
        patch: &Patch<'_, T>,
        w: W,
    ) -> io::Result<()> {
        PatchDisplay { f: self, patch }.write_into(w)
    }

    fn fmt_hunk<'a>(&'a self, hunk: &'a Hunk<'a, str>) -> impl Display + 'a {
        HunkDisplay { f: self, hunk }
    }

    #[cfg(feature = "std")]
    fn write_hunk_into<T: AsRef<[u8]> + ?Sized, W: io::Write>(
        &self,
        hunk: &Hunk<'_, T>,
        w: W,
    ) -> io::Result<()> {
        HunkDisplay { f: self, hunk }.write_into(w)
    }

    fn fmt_line<'a>(&'a self, line: &'a Line<'a, str>) -> impl Display + 'a {
        LineDisplay { f: self, line }
    }

    #[cfg(feature = "std")]
    fn write_line_into<T: AsRef<[u8]> + ?Sized, W: io::Write>(
        &self,
        line: &Line<'_, T>,
        w: W,
    ) -> io::Result<()> {
        LineDisplay { f: self, line }.write_into(w)
    }
}

impl Default for PatchFormatter {
    fn default() -> Self {
        Self::new()
    }
}

struct PatchDisplay<'a, T: ToOwned + ?Sized> {
    f: &'a PatchFormatter,
    patch: &'a Patch<'a, T>,
}

#[cfg(feature = "std")]
impl<T: ToOwned + AsRef<[u8]> + ?Sized> PatchDisplay<'_, T> {
    fn write_into<W: io::Write>(&self, mut w: W) -> io::Result<()> {
        if self.patch.original.is_some() || self.patch.modified.is_some() {
            #[cfg(feature = "color")]
            if self.f.with_color {
                write!(w, "{}", style::PATCH_HEADER)?;
            }
            if let Some(original) = &self.patch.original {
                write!(w, "--- ")?;
                original.write_into(&mut w)?;
                writeln!(w)?;
            }
            if let Some(modified) = &self.patch.modified {
                write!(w, "+++ ")?;
                modified.write_into(&mut w)?;
                writeln!(w)?;
            }
            #[cfg(feature = "color")]
            if self.f.with_color {
                write!(w, "{:#}", style::PATCH_HEADER)?;
            }
        }

        for hunk in &self.patch.hunks {
            self.f.write_hunk_into(hunk, &mut w)?;
        }

        Ok(())
    }
}

impl Display for PatchDisplay<'_, str> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.patch.original.is_some() || self.patch.modified.is_some() {
            #[cfg(feature = "color")]
            if self.f.with_color {
                write!(f, "{}", style::PATCH_HEADER)?;
            }
            if let Some(original) = &self.patch.original {
                writeln!(f, "--- {original}")?;
            }
            if let Some(modified) = &self.patch.modified {
                writeln!(f, "+++ {modified}")?;
            }
            #[cfg(feature = "color")]
            if self.f.with_color {
                write!(f, "{:#}", style::PATCH_HEADER)?;
            }
        }

        for hunk in &self.patch.hunks {
            write!(f, "{}", self.f.fmt_hunk(hunk))?;
        }

        Ok(())
    }
}

struct HunkDisplay<'a, T: ?Sized> {
    f: &'a PatchFormatter,
    hunk: &'a Hunk<'a, T>,
}

#[cfg(feature = "std")]
impl<T: AsRef<[u8]> + ?Sized> HunkDisplay<'_, T> {
    fn write_into<W: io::Write>(&self, mut w: W) -> io::Result<()> {
        #[cfg(feature = "color")]
        if self.f.with_color {
            write!(w, "{}", style::HUNK_HEADER)?;
        }
        write!(w, "@@ -{} +{} @@", self.hunk.old_range, self.hunk.new_range)?;
        #[cfg(feature = "color")]
        if self.f.with_color {
            write!(w, "{:#}", style::HUNK_HEADER)?;
        }

        if let Some(ctx) = self.hunk.function_context {
            write!(w, " ")?;
            #[cfg(feature = "color")]
            if self.f.with_color {
                write!(w, "{}", style::FUNCTION_CONTEXT)?;
            }
            write!(w, " ")?;
            w.write_all(ctx.as_ref())?;
            #[cfg(feature = "color")]
            if self.f.with_color {
                write!(w, "{:#}", style::FUNCTION_CONTEXT)?;
            }
        }
        writeln!(w)?;

        for line in &self.hunk.lines {
            self.f.write_line_into(line, &mut w)?;
        }

        Ok(())
    }
}

impl Display for HunkDisplay<'_, str> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        #[cfg(feature = "color")]
        if self.f.with_color {
            write!(f, "{}", style::HUNK_HEADER)?;
        }
        write!(f, "@@ -{} +{} @@", self.hunk.old_range, self.hunk.new_range)?;
        #[cfg(feature = "color")]
        if self.f.with_color {
            write!(f, "{:#}", style::HUNK_HEADER)?;
        }

        if let Some(ctx) = self.hunk.function_context {
            write!(f, " ")?;
            #[cfg(feature = "color")]
            if self.f.with_color {
                write!(f, "{}", style::FUNCTION_CONTEXT)?;
            }
            write!(f, " {ctx}")?;
            #[cfg(feature = "color")]
            if self.f.with_color {
                write!(f, "{:#}", style::FUNCTION_CONTEXT)?;
            }
        }
        writeln!(f)?;

        for line in &self.hunk.lines {
            write!(f, "{}", self.f.fmt_line(line))?;
        }

        Ok(())
    }
}

struct LineDisplay<'a, T: ?Sized> {
    f: &'a PatchFormatter,
    line: &'a Line<'a, T>,
}

#[cfg(feature = "std")]
impl<T: AsRef<[u8]> + ?Sized> LineDisplay<'_, T> {
    fn write_into<W: io::Write>(&self, mut w: W) -> io::Result<()> {
        #[cfg(feature = "color")]
        let (sign, line, style) = match self.line {
            Line::Context(line) => (' ', line.as_ref(), style::CONTEXT),
            Line::Delete(line) => ('-', line.as_ref(), style::DELETE),
            Line::Insert(line) => ('+', line.as_ref(), style::INSERT),
        };
        #[cfg(not(feature = "color"))]
        let (sign, line) = match self.line {
            Line::Context(line) => (' ', line.as_ref()),
            Line::Delete(line) => ('-', line.as_ref()),
            Line::Insert(line) => ('+', line.as_ref()),
        };

        #[cfg(feature = "color")]
        if self.f.with_color {
            write!(w, "{style}")?;
        }

        if self.f.suppress_blank_empty && sign == ' ' && line == b"\n" {
            w.write_all(line)?;
        } else {
            write!(w, "{sign}")?;
            w.write_all(line)?;
        }

        #[cfg(feature = "color")]
        if self.f.with_color {
            write!(w, "{style:#}")?;
        }

        if !line.ends_with(b"\n") {
            writeln!(w)?;
            if self.f.with_missing_newline_message {
                writeln!(w, "{NO_NEWLINE_AT_EOF}")?;
            }
        }

        Ok(())
    }
}

impl Display for LineDisplay<'_, str> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        #[cfg(feature = "color")]
        let (sign, line, style) = match self.line {
            Line::Context(line) => (' ', line, style::CONTEXT),
            Line::Delete(line) => ('-', line, style::DELETE),
            Line::Insert(line) => ('+', line, style::INSERT),
        };
        #[cfg(not(feature = "color"))]
        let (sign, line) = match self.line {
            Line::Context(line) => (' ', line),
            Line::Delete(line) => ('-', line),
            Line::Insert(line) => ('+', line),
        };

        #[cfg(feature = "color")]
        if self.f.with_color {
            write!(f, "{style}")?;
        }

        if self.f.suppress_blank_empty && sign == ' ' && *line == "\n" {
            write!(f, "{line}")?;
        } else {
            write!(f, "{sign}{line}")?;
        }

        #[cfg(feature = "color")]
        if self.f.with_color {
            write!(f, "{style:#}")?;
        }

        if !line.ends_with('\n') {
            writeln!(f)?;
            if self.f.with_missing_newline_message {
                writeln!(f, "{NO_NEWLINE_AT_EOF}")?;
            }
        }

        Ok(())
    }
}
