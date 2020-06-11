use super::*;
use crate::{
    diff::{Diff, DiffRange},
    patch::{apply, Patch},
    range::Range,
};

#[test]
fn diff_test2() {
    let a = "ABCABBA";
    let b = "CBABAC";
    let solution = diff(a, b);
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
    let solution = diff(a, b);
    assert_eq!(
        solution,
        vec![
            Diff::Delete("ab"),
            Diff::Equal("g"),
            Diff::Delete("def"),
            Diff::Insert("h"),
        ]
    );
}

#[test]
fn diff_test4() {
    let a = "bat";
    let b = "map";
    let solution = DiffOptions::default().diff_slice(a.as_bytes(), b.as_bytes());
    let expected: Vec<Diff<[u8]>> = vec![
        Diff::Delete(b"b"),
        Diff::Insert(b"m"),
        Diff::Equal(b"a"),
        Diff::Delete(b"t"),
        Diff::Insert(b"p"),
    ];
    assert_eq!(solution, expected);

    let solution = diff(a, b);
    assert_eq!(
        solution,
        vec![
            Diff::Delete("b"),
            Diff::Insert("m"),
            Diff::Equal("a"),
            Diff::Delete("t"),
            Diff::Insert("p"),
        ]
    );
}

#[test]
fn diff_test5() {
    let a = "abc";
    let b = "def";
    let solution = diff(a, b);
    assert_eq!(solution, vec![Diff::Delete("abc"), Diff::Insert("def")]);
}

#[test]
fn diff_test6() {
    let a = "ACZBDZ";
    let b = "ACBCBDEFD";
    let solution = diff(a, b);
    assert_eq!(
        solution,
        vec![
            Diff::Equal("AC"),
            Diff::Delete("Z"),
            Diff::Equal("B"),
            Diff::Insert("CBDEF"),
            Diff::Equal("D"),
            Diff::Delete("Z"),
        ]
    );
}

#[test]
fn diff_str() {
    let a = "A\nB\nC\nA\nB\nB\nA\n";
    let b = "C\nB\nA\nB\nA\nC\n";
    let patch = create_patch(a, b);
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

    assert_eq!(patch.to_string(), expected);
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

    let patch = opts.create_patch(lao, tzu);
    let patch_str = patch.to_string();
    assert_eq!(patch_str, expected);
    assert_eq!(Patch::from_str(expected).unwrap(), patch);
    assert_eq!(Patch::from_str(&patch_str).unwrap(), patch);
    assert_eq!(apply(lao, &patch), tzu);

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
    let patch = opts.create_patch(lao, tzu);
    let patch_str = patch.to_string();
    assert_eq!(patch_str, expected);
    assert_eq!(Patch::from_str(expected).unwrap(), patch);
    assert_eq!(Patch::from_str(&patch_str).unwrap(), patch);
    assert_eq!(apply(lao, &patch), tzu);

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
    let patch = opts.create_patch(lao, tzu);
    let patch_str = patch.to_string();
    assert_eq!(patch_str, expected);
    assert_eq!(Patch::from_str(expected).unwrap(), patch);
    assert_eq!(Patch::from_str(&patch_str).unwrap(), patch);
    assert_eq!(apply(lao, &patch), tzu);
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
    let patch = create_patch(old, new);
    let patch_str = patch.to_string();
    assert_eq!(patch_str, expected);
    assert_eq!(Patch::from_str(expected).unwrap(), patch);
    assert_eq!(Patch::from_str(&patch_str).unwrap(), patch);
    assert_eq!(apply(old, &patch), new);

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
    let patch = create_patch(old, new);
    let patch_str = patch.to_string();
    assert_eq!(patch_str, expected);
    assert_eq!(Patch::from_str(expected).unwrap(), patch);
    assert_eq!(Patch::from_str(&patch_str).unwrap(), patch);
    assert_eq!(apply(old, &patch), new);

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
    let patch = create_patch(old, new);
    let patch_str = patch.to_string();
    assert_eq!(patch_str, expected);
    assert_eq!(Patch::from_str(expected).unwrap(), patch);
    assert_eq!(Patch::from_str(&patch_str).unwrap(), patch);
    assert_eq!(apply(old, &patch), new);

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
    let patch = create_patch(old, new);
    let patch_str = patch.to_string();
    assert_eq!(patch_str, expected);
    assert_eq!(Patch::from_str(expected).unwrap(), patch);
    assert_eq!(Patch::from_str(&patch_str).unwrap(), patch);
    assert_eq!(apply(old, &patch), new);
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
    let a = "1A ";
    let b = "1A B A 2";
    let solution = diff(a, b);
    let expected = vec![Diff::Equal("1A "), Diff::Insert("B A 2")];
    assert_eq!(solution, expected);

    let mut to_comact = vec![
        DiffRange::Equal(Range::new(a, ..1), Range::new(b, ..1)),
        DiffRange::Insert(Range::new(b, 1..5)),
        DiffRange::Equal(Range::new(a, 1..), Range::new(b, 5..7)),
        DiffRange::Insert(Range::new(b, 7..)),
    ];

    cleanup::compact(&mut to_comact);
    let compacted: Vec<_> = to_comact.into_iter().map(Diff::from).collect();
    assert_eq!(compacted, expected);

    let a = "ACBD";
    let b = "ACBCBDEFD";
    let solution = diff(a, b);
    let expected = vec![Diff::Equal("ACB"), Diff::Insert("CBDEF"), Diff::Equal("D")];
    assert_eq!(solution, expected);

    let mut to_comact = vec![
        DiffRange::Equal(Range::new(a, ..2), Range::new(b, ..2)),
        DiffRange::Insert(Range::new(b, 2..4)),
        DiffRange::Equal(Range::new(a, 2..4), Range::new(b, 4..6)),
        DiffRange::Insert(Range::new(b, 6..)),
    ];

    cleanup::compact(&mut to_comact);
    let compacted: Vec<_> = to_comact.into_iter().map(Diff::from).collect();
    assert_eq!(compacted, expected);

    // actual: `[Equal("AC"), Delete("Z"), Insert("BC"), Equal("BD"), Delete("Z"), Insert("EFD")]`,
    // expected: `[Equal("AC"), Delete("Z"), Equal("B"), Insert("CBDEF"), Equal("D"), Delete("Z")]`', src/diff.rs:1094:9
}

#[test]
fn compact_new() {
    let a = "ACZBDZ";
    let b = "ACBCBDEFD";
    let expected = vec![
        Diff::Equal("AC"),
        Diff::Delete("Z"),
        Diff::Equal("B"),
        Diff::Insert("CBDEF"),
        Diff::Equal("D"),
        Diff::Delete("Z"),
    ];
    let mut to_comact = vec![
        DiffRange::Equal(Range::new(a, ..2), Range::new(b, ..2)),
        DiffRange::Delete(Range::new(a, 2..3)),
        DiffRange::Insert(Range::new(b, 2..4)),
        DiffRange::Equal(Range::new(a, 3..5), Range::new(b, 4..6)),
        DiffRange::Delete(Range::new(a, 5..6)),
        DiffRange::Insert(Range::new(b, 6..)),
    ];

    cleanup::compact(&mut to_comact);
    let compacted: Vec<_> = to_comact.iter().cloned().map(Diff::from).collect();
    assert_eq!(compacted, expected);

    // Flip it
    let expected = vec![
        Diff::Equal("AC"),
        Diff::Insert("Z"),
        Diff::Equal("B"),
        Diff::Delete("CBDEF"),
        Diff::Equal("D"),
        Diff::Insert("Z"),
    ];
    let mut to_comact = vec![
        DiffRange::Equal(Range::new(a, ..2), Range::new(b, ..2)),
        DiffRange::Insert(Range::new(a, 2..3)),
        DiffRange::Delete(Range::new(b, 2..4)),
        DiffRange::Equal(Range::new(a, 3..5), Range::new(b, 4..6)),
        DiffRange::Insert(Range::new(a, 5..6)),
        DiffRange::Delete(Range::new(b, 6..)),
    ];

    cleanup::compact(&mut to_comact);
    let compacted: Vec<_> = to_comact.iter().cloned().map(Diff::from).collect();
    assert_eq!(compacted, expected);
}
