use super::*;
use crate::{
    apply::apply,
    diff::{Diff, DiffRange},
    patch::Patch,
    range::Range,
    PatchFormatter,
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
            "\nexpected={:#?}\nactual={:#?}",
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
            "\nexpected={:#?}\nactual={:#?}",
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
fn without_no_newline_at_eof_message() {
    let old = "old line";
    let new = "new line";
    let expected = "\
--- original
+++ modified
@@ -1 +1 @@
-old line
+new line
";

    let f = PatchFormatter::new().missing_newline_message(false);
    let patch = create_patch(old, new);
    let bpatch = create_patch_bytes(old.as_bytes(), new.as_bytes());
    let patch_str = format!("{}", f.fmt_patch(&patch));
    let mut patch_bytes = Vec::new();
    f.write_patch_into(&bpatch, &mut patch_bytes).unwrap();

    assert_eq!(patch_str, expected);
    assert_eq!(patch_bytes, patch_str.as_bytes());
    assert_eq!(patch_bytes, expected.as_bytes());
    assert_eq!(apply(old, &patch).unwrap(), new);
    assert_eq!(
        crate::apply_bytes(old.as_bytes(), &bpatch).unwrap(),
        new.as_bytes()
    );
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
fn suppress_blank_empty() {
    let original = "\
1
2
3

4
";

    let modified = "\
1
2
3

5
";

    // Note that there is a space " " on the line after 3
    let expected = "\
--- original
+++ modified
@@ -2,4 +2,4 @@
 2
 3
 
-4
+5
";

    let f = PatchFormatter::new().suppress_blank_empty(false);
    let patch = create_patch(original, modified);
    let bpatch = create_patch_bytes(original.as_bytes(), modified.as_bytes());
    let patch_str = format!("{}", f.fmt_patch(&patch));
    let mut patch_bytes = Vec::new();
    f.write_patch_into(&bpatch, &mut patch_bytes).unwrap();

    assert_eq!(patch_str, expected);
    assert_eq!(patch_bytes, patch_str.as_bytes());
    assert_eq!(patch_bytes, expected.as_bytes());
    assert_eq!(apply(original, &patch).unwrap(), modified);
    assert_eq!(
        crate::apply_bytes(original.as_bytes(), &bpatch).unwrap(),
        modified.as_bytes()
    );

    // Note that there is no space " " on the line after 3
    let expected_suppressed = "\
--- original
+++ modified
@@ -2,4 +2,4 @@
 2
 3

-4
+5
";

    let f = PatchFormatter::new().suppress_blank_empty(true);
    let patch = create_patch(original, modified);
    let bpatch = create_patch_bytes(original.as_bytes(), modified.as_bytes());
    let patch_str = format!("{}", f.fmt_patch(&patch));
    let mut patch_bytes = Vec::new();
    f.write_patch_into(&bpatch, &mut patch_bytes).unwrap();

    assert_eq!(patch_str, expected_suppressed);
    assert_eq!(patch_bytes, patch_str.as_bytes());
    assert_eq!(patch_bytes, expected_suppressed.as_bytes());
    assert_eq!(apply(original, &patch).unwrap(), modified);
    assert_eq!(
        crate::apply_bytes(original.as_bytes(), &bpatch).unwrap(),
        modified.as_bytes()
    );
}

// Myers (heuristic) tests: verify that diffs produced by the default
// Myers algorithm are always applicable (i.e. the patch applied to the
// original yields the modified text), and that switching to
// `DiffAlgorithm::Minimal` always produces a minimal diff.

/// Apply `patch` produced from `original` and assert it yields `modified`.
fn assert_roundtrip(original: &str, modified: &str, patch: &Patch<str>) {
    let result = apply(original, patch).unwrap();
    assert_eq!(result, modified, "patch did not round-trip correctly");
}

/// Count the `+`/`-` lines (insertions and deletions) in a unified
/// patch, excluding the `+++`/`---` file headers.
fn edit_line_count(patch: &Patch<str>) -> usize {
    use crate::patch::Line;
    patch
        .hunks()
        .iter()
        .flat_map(|h| h.lines().iter())
        .filter(|l| matches!(l, Line::Insert(_) | Line::Delete(_)))
        .count()
}

#[test]
fn myers_default_small_inputs_match_minimal() {
    // For small inputs the heuristic should never fire, so the Myers
    // and Minimal algorithms must agree byte-for-byte.
    let cases = [
        ("ABCABBA", "CBABAC"),
        ("bat", "map"),
        ("abc", "def"),
        ("abgdef", "gh"),
    ];
    for (a, b) in &cases {
        let minimal = DiffOptions::new()
            .set_algorithm(DiffAlgorithm::Minimal)
            .create_patch(a, b);
        let myers = DiffOptions::new()
            .set_algorithm(DiffAlgorithm::Myers)
            .create_patch(a, b);
        assert_eq!(
            minimal.to_string(),
            myers.to_string(),
            "small input should produce identical patch: ({a:?}, {b:?})"
        );
    }
}

#[test]
fn myers_block_swap_roundtrips() {
    // The block-swap case from the `myers_diffy_vs_git` test.
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
    let modified = "\
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
    let patch = DiffOptions::new().create_patch(original, modified);
    assert_roundtrip(original, modified, &patch);
}

#[test]
fn myers_empty_inputs() {
    // Both empty.
    let patch = DiffOptions::new().create_patch("", "");
    assert!(patch.hunks().is_empty());

    // One empty.
    let patch = DiffOptions::new().create_patch("", "new content\n");
    assert_roundtrip("", "new content\n", &patch);
    let patch = DiffOptions::new().create_patch("old content\n", "");
    assert_roundtrip("old content\n", "", &patch);
}

#[test]
fn minimal_is_not_larger_than_myers_on_small_input() {
    // On small inputs the heuristic never fires, so Myers and Minimal
    // should agree exactly.
    let original: String = (0..40).map(|i| format!("line {i}\n")).collect();
    let modified: String = (0..40)
        .map(|i| {
            if i % 7 == 0 {
                format!("changed {i}\n")
            } else {
                format!("line {i}\n")
            }
        })
        .collect();

    let myers = DiffOptions::new()
        .set_algorithm(DiffAlgorithm::Myers)
        .create_patch(&original, &modified);
    let minimal = DiffOptions::new()
        .set_algorithm(DiffAlgorithm::Minimal)
        .create_patch(&original, &modified);

    assert_roundtrip(&original, &modified, &myers);
    assert_roundtrip(&original, &modified, &minimal);
    assert_eq!(
        myers.to_string(),
        minimal.to_string(),
        "small input should produce identical output under both algorithms",
    );
}

#[test]
fn minimal_is_smaller_than_myers_when_heuristic_fires() {
    // Pathological input modeled on the test case in
    // minimal_no_spurious_edits_on_repeated_prefix,
    // scaled up so the edit cost exceeds the heuristic budget
    // and the bail has a reason to fire. Two large disjoint blocks of
    // unique lines flank a shared block — Minimal pivots on the shared
    // block and emits two clean hunks; Myers bails out before finding
    // the shared block and emits a single mega-hunk that touches the
    // entire file.
    //
    // Invariants under test:
    //   1. Both algorithms produce patches that round-trip.
    //   2. Minimal's `+`/`-` count is strictly less than Myers'.
    //   3. The two patches are textually distinguishable.
    // (3) is the load-bearing assertion: without it, a future change
    // that accidentally short-circuits the heuristic would make the
    // test pass silently.
    let mut original = String::new();
    for i in 0..200 {
        original.push_str(&format!("pre-A-{i}\n"));
    }
    for i in 0..50 {
        original.push_str(&format!("common-{i}\n"));
    }
    for i in 0..200 {
        original.push_str(&format!("pre-B-{i}\n"));
    }

    let mut modified = String::new();
    for i in 0..200 {
        modified.push_str(&format!("post-A-{i}\n"));
    }
    for i in 0..50 {
        modified.push_str(&format!("common-{i}\n"));
    }
    for i in 0..200 {
        modified.push_str(&format!("post-B-{i}\n"));
    }

    let myers = DiffOptions::new()
        .set_algorithm(DiffAlgorithm::Myers)
        .create_patch(&original, &modified);
    let minimal = DiffOptions::new()
        .set_algorithm(DiffAlgorithm::Minimal)
        .create_patch(&original, &modified);

    assert_roundtrip(&original, &modified, &myers);
    assert_roundtrip(&original, &modified, &minimal);

    let myers_edits = edit_line_count(&myers);
    let minimal_edits = edit_line_count(&minimal);
    assert!(
        minimal_edits < myers_edits,
        "Minimal edits ({minimal_edits}) should be less than Myers \
         edits ({myers_edits}) on an input designed to trigger the \
         heuristic",
    );
    assert_ne!(
        myers.to_string(),
        minimal.to_string(),
        "Myers and Minimal should produce textually different patches \
         when the heuristic fires",
    );
}

#[test]
fn myers_heuristic_handles_asymmetric_inputs() {
    // Regression test for max_cost-bailout overflow bugs on asymmetric
    // inputs (`n != m`). Each case flanks a shared block with large
    // asymmetric unique-line blocks on either side; the edit cost
    // comfortably exceeds the heuristic budget at multiple recursion
    // levels.
    for (n, m, mid) in
        [(600, 200, 50), (200, 600, 50), (800, 100, 20), (100, 800, 20)]
    {
        let mut original = String::new();
        for i in 0..n {
            original.push_str(&format!("pre-A-{i}\n"));
        }
        for i in 0..mid {
            original.push_str(&format!("common-{i}\n"));
        }
        for i in 0..n {
            original.push_str(&format!("pre-B-{i}\n"));
        }

        let mut modified = String::new();
        for i in 0..m {
            modified.push_str(&format!("post-A-{i}\n"));
        }
        for i in 0..mid {
            modified.push_str(&format!("common-{i}\n"));
        }
        for i in 0..m {
            modified.push_str(&format!("post-B-{i}\n"));
        }

        for algorithm in [DiffAlgorithm::Myers, DiffAlgorithm::Minimal] {
            let patch = DiffOptions::new()
                .set_algorithm(algorithm)
                .create_patch(&original, &modified);
            let applied = apply(&original, &patch).unwrap_or_else(|e| {
                panic!(
                    "apply failed for {algorithm:?} with n={n}, m={m}, \
                     mid={mid}: {e}",
                )
            });
            assert_eq!(
                applied, modified,
                "{algorithm:?} patch did not round-trip for n={n}, \
                 m={m}, mid={mid}",
            );
        }
    }
}

#[test]
fn minimal_no_spurious_edits_on_repeated_prefix() {
    let pre = "x\nx\nx\nx\n";
    let post = "x\nx\nx\nA\nB\nC\nD\nx\nE\nF\nG\n";

    for algorithm in [DiffAlgorithm::Myers, DiffAlgorithm::Minimal] {
        let patch = DiffOptions::new()
            .set_algorithm(algorithm)
            .create_patch(pre, post);
        assert_roundtrip(pre, post, &patch);

        // No line should be `-x` or `+x` — all the `x`s must be
        // emitted as context.
        let rendered = patch.to_string();
        for (lineno, line) in rendered.lines().enumerate() {
            assert!(
                line != "-x" && line != "+x",
                "{algorithm:?} produced spurious `x` edit at line \
                 {lineno}: {rendered}",
            );
        }
    }
}

/// For any pair of small ASCII strings, both `Myers` and `Minimal`
/// must produce a patch that round-trips. This catches coordinate
/// arithmetic bugs in the heuristic bailouts that hand-written
/// cases would miss.
#[test_strategy::proptest]
fn myers_and_minimal_roundtrip_arbitrary(
    #[strategy("[a-z\n]{0,200}")] original: String,
    #[strategy("[a-z\n]{0,200}")] modified: String,
) {
    let myers = DiffOptions::new()
        .set_algorithm(DiffAlgorithm::Myers)
        .create_patch(&original, &modified);
    let applied = apply(&original, &myers).expect("Myers patch must apply");
    proptest::prop_assert_eq!(applied, modified.clone(), "Myers patch did not round-trip");

    let minimal = DiffOptions::new()
        .set_algorithm(DiffAlgorithm::Minimal)
        .create_patch(&original, &modified);
    let applied = apply(&original, &minimal).expect("Minimal patch must apply");
    proptest::prop_assert_eq!(applied, modified, "Minimal patch did not round-trip");
}

/// Same round-trip invariant on the bytes path.
#[test_strategy::proptest]
fn myers_bytes_roundtrip_arbitrary(
    #[strategy(proptest::collection::vec(0u8..=255, 0..200))] original: Vec<u8>,
    #[strategy(proptest::collection::vec(0u8..=255, 0..200))] modified: Vec<u8>,
) {
    let patch = DiffOptions::new()
        .set_algorithm(DiffAlgorithm::Myers)
        .create_patch_bytes(&original, &modified);
    let applied =
        crate::apply_bytes(&original, &patch).expect("Myers bytes patch must apply");
    proptest::prop_assert_eq!(applied, modified, "Myers bytes patch did not round-trip");
}

// In the event that a patch has an invalid hunk range we want to ensure that when apply is
// attempting to search for a matching position to apply a hunk that the search algorithm runs in
// time bounded by the length of the original image being patched. Before clamping the search space
// this test would take >200ms and now it runs in roughly ~30us on an M1 laptop.
#[test]
fn apply_with_incorrect_hunk_has_bounded_performance() {
    let patch = "\
@@ -10,6 +1000000,8 @@
 First:
     Life before death,
     strength before weakness,
     journey before destination.
 Second:
-    I will put the law before all else.
+    I swear to seek justice,
+    to let it guide me,
+    until I find a more perfect Ideal.
";

    let original = "\
First:
    Life before death,
    strength before weakness,
    journey before destination.
Second:
    I will put the law before all else.
";

    let expected = "\
First:
    Life before death,
    strength before weakness,
    journey before destination.
Second:
    I swear to seek justice,
    to let it guide me,
    until I find a more perfect Ideal.
";

    let patch = Patch::from_str(patch).unwrap();

    let now = std::time::Instant::now();

    let result = apply(original, &patch).unwrap();

    let elapsed = now.elapsed();

    println!("{:?}", elapsed);
    assert!(elapsed < std::time::Duration::from_micros(200));

    assert_eq!(result, expected);
}

#[test]
fn reverse_empty_file() {
    let p = create_patch("", "make it so");
    let reverse = p.reverse();

    let hunk_lines = p.hunks().iter().map(|h| h.lines());
    let reverse_hunk_lines = reverse.hunks().iter().map(|h| h.lines());

    for (lines, reverse_lines) in hunk_lines.zip(reverse_hunk_lines) {
        for (line, reverse) in lines.iter().zip(reverse_lines.iter()) {
            match line {
                l @ Line::Context(_) => assert_eq!(l, reverse),
                Line::Delete(d) => assert!(matches!(reverse, Line::Insert(i) if d == i)),
                Line::Insert(i) => assert!(matches!(reverse, Line::Delete(d) if d == i)),
            }
        }
    }

    let re_reverse = apply(&apply("", &p).unwrap(), &reverse).unwrap();
    assert_eq!(re_reverse, "");
}

#[test]
fn reverse_multi_line_file() {
    let original = r"Commander Worf
What do you want this time, Picard?!
Commander Worf how dare you speak to mean that way!
";
    let modified = r"Commander Worf
Yes, Captain Picard?
Commander Worf, you are a valued member of my crew
Why, thank you Captain.  As are you.  A true warrior. Kupluh!
Kupluh, Indeed
";

    let p = create_patch(original, modified);
    let reverse = p.reverse();

    let re_reverse = apply(&apply(original, &p).unwrap(), &reverse).unwrap();
    assert_eq!(re_reverse, original);
}
