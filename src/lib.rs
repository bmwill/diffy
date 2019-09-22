//! Tools for finding and manipulating differences between files

pub mod diff;

#[cfg(test)]
mod tests {
    use crate::diff::diff;

    #[test]
    fn diff_test() {
        diff(b"A\nB\nC\nA\nB\nB\nA", b"C\nB\nA\nB\nA\nC");
    }
}
