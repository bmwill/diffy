use super::*;
use crate::{
    apply::apply,
    diff::{Diff, DiffRange},
    patch::Patch,
    range::Range,
};

// Helper macros are based off of the ones used in [dissimilar](https://docs.rs/dissimilar)
macro_rules! diff_range_list {
    () => {
        Vec::new()
    };
    ($($kind:ident($text:literal)),+ $(,)?) => {{
        macro_rules! text1 {
            (Insert, $s:literal) => { "" };
            (Delete, $s:literal) => { $s };
            (Equal, $s:literal) => { $s };
        }
        macro_rules! text2 {
            (Insert, $s:literal) => { $s };
            (Delete, $s:literal) => { "" };
            (Equal, $s:literal) => { $s };
        }
        let _text1 = concat!($(text1!($kind, $text)),*);
        let _text2 = concat!($(text2!($kind, $text)),*);
        let (_i, _j) = (&mut 0, &mut 0);
        macro_rules! range {
            (Insert, $s:literal) => {
                DiffRange::Insert(range(_text2, _j, $s))
            };
            (Delete, $s:literal) => {
                DiffRange::Delete(range(_text1, _i, $s))
            };
            (Equal, $s:literal) => {
                DiffRange::Equal(range(_text1, _i, $s), range(_text2, _j, $s))
            };
        }
        vec![$(range!($kind, $text)),*]
    }};
}

fn range<'a>(doc: &'a str, offset: &mut usize, text: &str) -> Range<'a, str> {
    let range = Range::new(doc, *offset..*offset + text.len());
    *offset += text.len();
    range
}

macro_rules! assert_diff_range {
    ([$($kind:ident($text:literal)),* $(,)?], $solution:ident $(,)?) => {
        let expected = &[$(Diff::$kind($text)),*];
        assert!(
            same_diffs(expected, &$solution),
            concat!("\nexpected={:#?}\nactual={:#?}"),
            expected, $solution,
        );
    };
    ([$($kind:ident($text:literal)),* $(,)?], $solution:ident, $msg:expr $(,)?) => {
        let expected = &[$(Diff::$kind($text)),*];
        assert!(
            same_diffs(expected, &$solution),
            concat!($msg, "\nexpected={:#?}\nactual={:#?}"),
            expected, $solution,
        );
    };
}

fn same_diffs(expected: &[Diff<str>], actual: &[DiffRange<str>]) -> bool {
    expected.len() == actual.len()
        && expected.iter().zip(actual).all(|pair| match pair {
            (Diff::Insert(expected), DiffRange::Insert(actual)) => *expected == actual.as_slice(),
            (Diff::Delete(expected), DiffRange::Delete(actual)) => *expected == actual.as_slice(),
            (Diff::Equal(expected), DiffRange::Equal(actual1, actual2)) => {
                *expected == actual1.as_slice() && *expected == actual2.as_slice()
            }
            (_, _) => false,
        })
}

macro_rules! assert_diff {
    ([$($kind:ident($text:literal)),* $(,)?], $solution:ident $(,)?) => {
        let expected: &[_] = &[$(Diff::$kind($text)),*];
        assert_eq!(
            expected,
            &$solution[..],
            concat!("\nexpected={:#?}\nactual={:#?}"),
            expected, $solution,
        );
    };
    ([$($kind:ident($text:literal)),* $(,)?], $solution:ident, $msg:expr $(,)?) => {
        let expected: &[_] = &[$(Diff::$kind($text)),*];
        assert_eq!(
            expected,
            &$solution[..],
            concat!($msg, "\nexpected={:#?}\nactual={:#?}"),
            expected, $solution,
        );
    };
}

#[test]
fn test_diff_str() {
    let a = "ABCABBA";
    let b = "CBABAC";
    let solution = diff(a, b);
    assert_diff!(
        [
            Delete("AB"),
            Equal("C"),
            Delete("A"),
            Equal("B"),
            Insert("A"),
            Equal("BA"),
            Insert("C"),
        ],
        solution,
    );

    let a = "abgdef";
    let b = "gh";
    let solution = diff(a, b);
    assert_diff!(
        [Delete("ab"), Equal("g"), Delete("def"), Insert("h")],
        solution,
    );

    let a = "bat";
    let b = "map";
    let solution = diff(a, b);
    assert_diff!(
        [
            Delete("b"),
            Insert("m"),
            Equal("a"),
            Delete("t"),
            Insert("p"),
        ],
        solution,
    );

    let a = "ACZBDZ";
    let b = "ACBCBDEFD";
    let solution = diff(a, b);
    assert_diff!(
        [
            Equal("AC"),
            Delete("Z"),
            Equal("B"),
            Insert("CBDEF"),
            Equal("D"),
            Delete("Z"),
        ],
        solution,
    );

    let a = "1A ";
    let b = "1A B A 2";
    let solution = diff(a, b);
    assert_diff!([Equal("1A "), Insert("B A 2")], solution);

    let a = "ACBD";
    let b = "ACBCBDEFD";
    let solution = diff(a, b);
    assert_diff!([Equal("ACB"), Insert("CBDEF"), Equal("D")], solution);

    let a = "abc";
    let b = "def";
    let solution = diff(a, b);
    assert_diff!([Delete("abc"), Insert("def")], solution, "No Equal");
}

#[test]
fn test_diff_slice() {
    let a = b"bat";
    let b = b"map";
    let solution = DiffOptions::default().diff_slice(a, b);
    let solution: Vec<_> = solution.into_iter().map(Diff::from).collect();
    let expected: Vec<Diff<[u8]>> = vec![
        Diff::Delete(b"b"),
        Diff::Insert(b"m"),
        Diff::Equal(b"a"),
        Diff::Delete(b"t"),
        Diff::Insert(b"p"),
    ];
    assert_eq!(solution, expected);
}

#[test]
fn test_unicode() {
    // Unicode snowman and unicode comet have the same first two bytes. A
    // byte-based diff would produce a 2-byte Equal followed by 1-byte Delete
    // and Insert.
    let snowman = "\u{2603}";
    let comet = "\u{2604}";
    assert_eq!(snowman.as_bytes()[..2], comet.as_bytes()[..2]);

    let d = diff(snowman, comet);
    assert_eq!(d, vec![Diff::Delete(snowman), Diff::Insert(comet)]);
}

#[test]
fn test_compact() {
    let mut solution = diff_range_list![];
    cleanup::compact(&mut solution);
    assert_diff_range!([], solution, "Null case");

    let mut solution = diff_range_list![Equal("a"), Delete("b"), Insert("c")];
    cleanup::compact(&mut solution);
    assert_diff_range!(
        [Equal("a"), Delete("b"), Insert("c")],
        solution,
        "No change case",
    );

    // TODO implement equality compaction
    // let mut solution = diff_range_list![Equal("a"), Equal("b"), Equal("c")];
    // cleanup::compact(&mut solution);
    // assert_diff_range!([Equal("abc")], solution, "Compact equalities");

    let mut solution = diff_range_list![Delete("a"), Delete("b"), Delete("c")];
    cleanup::compact(&mut solution);
    assert_diff_range!([Delete("abc")], solution, "Compact deletions");

    let mut solution = diff_range_list![Insert("a"), Insert("b"), Insert("c")];
    cleanup::compact(&mut solution);
    assert_diff_range!([Insert("abc")], solution, "Compact Insertions");

    let mut solution = diff_range_list![
        Delete("a"),
        Insert("b"),
        Delete("c"),
        Insert("d"),
        Equal("ef"),
    ];
    cleanup::compact(&mut solution);
    assert_diff_range!(
        [Delete("ac"), Insert("bd"), Equal("ef")],
        solution,
        "Compact interweave",
    );

    let mut solution = diff_range_list![
        Equal("a"),
        Delete("b"),
        Equal("c"),
        Delete("ac"),
        Equal("x"),
    ];
    cleanup::compact(&mut solution);
    assert_diff_range!(
        [Equal("a"), Delete("bca"), Equal("cx")],
        solution,
        "Slide edit left",
    );

    let mut solution = diff_range_list![
        Equal("x"),
        Delete("ca"),
        Equal("c"),
        Delete("b"),
        Equal("a"),
    ];
    cleanup::compact(&mut solution);
    assert_diff_range!([Equal("xca"), Delete("cba")], solution, "Slide edit right");

    let mut solution = diff_range_list![Equal(""), Insert("a"), Equal("b")];
    cleanup::compact(&mut solution);
    assert_diff_range!([Insert("a"), Equal("b")], solution, "Empty equality");

    let mut solution = diff_range_list![Equal("1"), Insert("A B "), Equal("A "), Insert("2")];

    cleanup::compact(&mut solution);
    assert_diff_range!([Equal("1A "), Insert("B A 2")], solution);

    let mut solution = diff_range_list![Equal("AC"), Insert("BC"), Equal("BD"), Insert("EFD")];
    cleanup::compact(&mut solution);

    assert_diff_range!([Equal("ACB"), Insert("CBDEF"), Equal("D")], solution);

    let mut solution = diff_range_list![
        Equal("AC"),
        Delete("Z"),
        Insert("BC"),
        Equal("BD"),
        Delete("Z"),
        Insert("EFD"),
    ];

    cleanup::compact(&mut solution);
    assert_diff_range!(
        [
            Equal("AC"),
            Delete("Z"),
            Equal("B"),
            Insert("CBDEF"),
            Equal("D"),
            Delete("Z"),
        ],
        solution,
        "Compact Inserts"
    );

    let mut solution = diff_range_list![
        Equal("AC"),
        Insert("Z"),
        Delete("BC"),
        Equal("BD"),
        Insert("Z"),
        Delete("EFD"),
    ];
    cleanup::compact(&mut solution);
    assert_diff_range!(
        [
            Equal("AC"),
            Insert("Z"),
            Equal("B"),
            Delete("CBDEF"),
            Equal("D"),
            Insert("Z"),
        ],
        solution,
        "Compact Deletions"
    );
}

macro_rules! assert_patch {
    ($diff_options:expr, $old:ident, $new:ident, $expected:ident $(,)?) => {
        let patch = $diff_options.create_patch($old, $new);
        let bpatch = $diff_options.create_patch_bytes($old.as_bytes(), $new.as_bytes());
        let patch_str = patch.to_string();
        let patch_bytes = bpatch.to_bytes();
        assert_eq!(patch_str, $expected);
        assert_eq!(patch_bytes, patch_str.as_bytes());
        assert_eq!(patch_bytes, $expected.as_bytes());
        assert_eq!(Patch::from_str($expected).unwrap(), patch);
        assert_eq!(Patch::from_str(&patch_str).unwrap(), patch);
        assert_eq!(Patch::from_bytes($expected.as_bytes()).unwrap(), bpatch);
        assert_eq!(Patch::from_bytes(&patch_bytes).unwrap(), bpatch);
        assert_eq!(apply($old, &patch).unwrap(), $new);
        assert_eq!(
            crate::apply_bytes($old.as_bytes(), &bpatch).unwrap(),
            $new.as_bytes()
        );
        assert_eq!(
            crate::apply_all_bytes($old.as_bytes(), &bpatch, crate::ApplyOptions::new()).0,
            $new.as_bytes()
        );
        assert_eq!(
            crate::apply_all_bytes(
                $old.as_bytes(),
                &bpatch,
                crate::ApplyOptions::new().with_max_fuzzy(1)
            )
            .0,
            $new.as_bytes()
        );
        assert_eq!(
            crate::apply_all($old, &patch, crate::ApplyOptions::new()).0,
            $new
        );
        assert_eq!(
            crate::apply_all($old, &patch, crate::ApplyOptions::new().with_max_fuzzy(1)).0,
            $new
        );
    };
    ($old:ident, $new:ident, $expected:ident $(,)?) => {
        assert_patch!(DiffOptions::default(), $old, $new, $expected);
    };
}

#[test]
fn diff_str() {
    let a = "A\nB\nC\nA\nB\nB\nA\n";
    let b = "C\nB\nA\nB\nA\nC\n";
    let expected = "\
--- original
+++ modified
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

    assert_patch!(a, b, expected);
}

#[test]
fn sample() {
    let mut opts = DiffOptions::default();
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
--- original
+++ modified
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

    assert_patch!(opts, lao, tzu, expected);

    let expected = "\
--- original
+++ modified
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
    opts.set_context_len(0);
    assert_patch!(opts, lao, tzu, expected);

    let expected = "\
--- original
+++ modified
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
    opts.set_context_len(1);
    assert_patch!(opts, lao, tzu, expected);
}

#[test]
fn no_newline_at_eof() {
    let old = "old line";
    let new = "new line";
    let expected = "\
--- original
+++ modified
@@ -1 +1 @@
-old line
\\ No newline at end of file
+new line
\\ No newline at end of file
";
    assert_patch!(old, new, expected);

    let old = "old line\n";
    let new = "new line";
    let expected = "\
--- original
+++ modified
@@ -1 +1 @@
-old line
+new line
\\ No newline at end of file
";
    assert_patch!(old, new, expected);

    let old = "old line";
    let new = "new line\n";
    let expected = "\
--- original
+++ modified
@@ -1 +1 @@
-old line
\\ No newline at end of file
+new line
";
    assert_patch!(old, new, expected);

    let old = "old line\ncommon line";
    let new = "new line\ncommon line";
    let expected = "\
--- original
+++ modified
@@ -1,2 +1,2 @@
-old line
+new line
 common line
\\ No newline at end of file
";
    assert_patch!(old, new, expected);
}

#[test]
fn myers_diffy_vs_git() {
    let original = "\
void Chunk_copy(Chunk *src, size_t src_start, Chunk *dst, size_t dst_start, size_t n)
{
    if (!Chunk_bounds_check(src, src_start, n)) return;
    if (!Chunk_bounds_check(dst, dst_start, n)) return;

    memcpy(dst->data + dst_start, src->data + src_start, n);
}

int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
{
    if (chunk == NULL) return 0;

    return start <= chunk->length && n <= chunk->length - start;
}
";
    let a = "\
int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
{
    if (chunk == NULL) return 0;

    return start <= chunk->length && n <= chunk->length - start;
}

void Chunk_copy(Chunk *src, size_t src_start, Chunk *dst, size_t dst_start, size_t n)
{
    if (!Chunk_bounds_check(src, src_start, n)) return;
    if (!Chunk_bounds_check(dst, dst_start, n)) return;

    memcpy(dst->data + dst_start, src->data + src_start, n);
}
";

    // TODO This differs from the expected output when using git's myers algorithm
    let expected_git = "\
--- original
+++ modified
@@ -1,14 +1,14 @@
-void Chunk_copy(Chunk *src, size_t src_start, Chunk *dst, size_t dst_start, size_t n)
+int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
 {
-    if (!Chunk_bounds_check(src, src_start, n)) return;
-    if (!Chunk_bounds_check(dst, dst_start, n)) return;
+    if (chunk == NULL) return 0;

-    memcpy(dst->data + dst_start, src->data + src_start, n);
+    return start <= chunk->length && n <= chunk->length - start;
 }

-int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
+void Chunk_copy(Chunk *src, size_t src_start, Chunk *dst, size_t dst_start, size_t n)
 {
-    if (chunk == NULL) return 0;
+    if (!Chunk_bounds_check(src, src_start, n)) return;
+    if (!Chunk_bounds_check(dst, dst_start, n)) return;

-    return start <= chunk->length && n <= chunk->length - start;
+    memcpy(dst->data + dst_start, src->data + src_start, n);
 }
";
    let git_patch = Patch::from_str(expected_git).unwrap();
    assert_eq!(apply(original, &git_patch).unwrap(), a);

    let expected_diffy = "\
--- original
+++ modified
@@ -1,3 +1,10 @@
+int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
+{
+    if (chunk == NULL) return 0;
+
+    return start <= chunk->length && n <= chunk->length - start;
+}
+
 void Chunk_copy(Chunk *src, size_t src_start, Chunk *dst, size_t dst_start, size_t n)
 {
     if (!Chunk_bounds_check(src, src_start, n)) return;
@@ -5,10 +12,3 @@

     memcpy(dst->data + dst_start, src->data + src_start, n);
 }
-
-int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
-{
-    if (chunk == NULL) return 0;
-
-    return start <= chunk->length && n <= chunk->length - start;
-}
";
    assert_patch!(original, a, expected_diffy);
}

#[test]
fn fuzzy_patch() {
    let diff = Patch::from_str(
        "\
--- original
+++ modified
@@ -1,6 +1,6 @@
 A
 B
-C
-D
+E
+F
 G
 H
",
    )
    .unwrap();
    let newer = "0\nB\nC\nD\nG\nH\n";
    let expected = "0\nB\nE\nF\nG\nH\n";
    println!("{}", diff);
    assert_eq!(
        crate::apply_all(newer, &diff, crate::ApplyOptions::new().with_max_fuzzy(2)),
        (expected.to_owned(), vec![]),
    )
}
