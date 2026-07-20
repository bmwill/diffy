use super::*;

macro_rules! assert_merge {
    ($original:ident, $ours:ident, $theirs:ident, $kind:ident($expected:expr_2021), $msg:literal $(,)?) => {
        let solution = merge($original, $ours, $theirs);

        macro_rules! result {
            (Ok, $s:expr_2021) => {
                Result::<&str, &str>::Ok($s)
            };
            (Err, $s:expr_2021) => {
                Result::<&str, &str>::Err($s)
            };
        }
        assert!(
            same_merge(result!($kind, $expected), &solution),
            concat!($msg, "\nexpected={:#?}\nactual={:#?}"),
            result!($kind, $expected),
            solution
        );

        let solution_bytes =
            merge_bytes($original.as_bytes(), $ours.as_bytes(), $theirs.as_bytes());

        macro_rules! result_bytes {
            (Ok, $s:expr_2021) => {
                Result::<&[u8], &[u8]>::Ok($s.as_bytes())
            };
            (Err, $s:expr_2021) => {
                Result::<&[u8], &[u8]>::Err($s.as_bytes())
            };
        }
        assert!(
            same_merge_bytes(result_bytes!($kind, $expected), &solution_bytes),
            concat!($msg, "\nexpected={:#?}\nactual={:#?}"),
            result_bytes!($kind, $expected),
            solution_bytes
        );
    };
}

fn same_merge(expected: Result<&str, &str>, actual: &Result<String, String>) -> bool {
    match (expected, actual) {
        (Ok(expected), Ok(actual)) => expected == actual,
        (Err(expected), Err(actual)) => expected == actual,
        (_, _) => false,
    }
}

fn same_merge_bytes(expected: Result<&[u8], &[u8]>, actual: &Result<Vec<u8>, Vec<u8>>) -> bool {
    match (expected, actual) {
        (Ok(expected), Ok(actual)) => expected == &actual[..],
        (Err(expected), Err(actual)) => expected == &actual[..],
        (_, _) => false,
    }
}

#[test]
fn test_merge() {
    let original = "\
carrots
garlic
onions
salmon
mushrooms
tomatoes
salt
";
    let a = "\
carrots
salmon
mushrooms
tomatoes
garlic
onions
salt
";
    let b = "\
carrots
salmon
garlic
onions
mushrooms
tomatoes
salt
";

    assert_merge!(original, original, original, Ok(original), "Equal case #1");
    assert_merge!(original, a, a, Ok(a), "Equal case #2");
    assert_merge!(original, b, b, Ok(b), "Equal case #3");

    let expected = "\
carrots
<<<<<<< ours
salmon
||||||| original
garlic
onions
salmon
=======
salmon
garlic
onions
>>>>>>> theirs
mushrooms
tomatoes
garlic
onions
salt
";

    assert_merge!(original, a, b, Err(expected), "Single Conflict case");

    let expected = "\
carrots
<<<<<<< ours
salmon
garlic
onions
||||||| original
garlic
onions
salmon
=======
salmon
>>>>>>> theirs
mushrooms
tomatoes
garlic
onions
salt
";

    assert_merge!(
        original,
        b,
        a,
        Err(expected),
        "Reverse Single Conflict case"
    );

    let original = "\
carrots
garlic
onions
salmon
tomatoes
salt
";
    let a = "\
carrots
salmon
tomatoes
garlic
onions
salt
";
    let b = "\
carrots
salmon
garlic
onions
tomatoes
salt
";
    let expected = "\
carrots
<<<<<<< ours
salmon
tomatoes
||||||| original
=======
salmon
>>>>>>> theirs
garlic
onions
<<<<<<< ours
||||||| original
salmon
tomatoes
=======
tomatoes
>>>>>>> theirs
salt
";

    assert_merge!(original, a, b, Err(expected), "Multiple Conflict case");

    let expected = "\
carrots
<<<<<<< ours
salmon
||||||| original
=======
salmon
tomatoes
>>>>>>> theirs
garlic
onions
<<<<<<< ours
tomatoes
||||||| original
salmon
tomatoes
=======
>>>>>>> theirs
salt
";
    assert_merge!(
        original,
        b,
        a,
        Err(expected),
        "Reverse Multiple Conflict case"
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
    let b = "\
void Chunk_copy(Chunk *src, size_t src_start, Chunk *dst, size_t dst_start, size_t n)
{
    if (!Chunk_bounds_check(src, src_start, n)) return;
    if (!Chunk_bounds_check(dst, dst_start, n)) return;

    // copy the bytes
    memcpy(dst->data + dst_start, src->data + src_start, n);
}

int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
{
    if (chunk == NULL) return 0;

    return start <= chunk->length && n <= chunk->length - start;
}
";

    // TODO investigate why this doesn't match git's output
    let _expected_git = "\
int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
{
    if (chunk == NULL) return 0;

<<<<<<< ours
    return start <= chunk->length && n <= chunk->length - start;
||||||| original
    memcpy(dst->data + dst_start, src->data + src_start, n);
=======
    // copy the bytes
    memcpy(dst->data + dst_start, src->data + src_start, n);
>>>>>>> theirs
}

void Chunk_copy(Chunk *src, size_t src_start, Chunk *dst, size_t dst_start, size_t n)
{
    if (!Chunk_bounds_check(src, src_start, n)) return;
    if (!Chunk_bounds_check(dst, dst_start, n)) return;

    memcpy(dst->data + dst_start, src->data + src_start, n);
}
";

    let expected_diffy = "\
int Chunk_bounds_check(Chunk *chunk, size_t start, size_t n)
{
    if (chunk == NULL) return 0;

    return start <= chunk->length && n <= chunk->length - start;
}

void Chunk_copy(Chunk *src, size_t src_start, Chunk *dst, size_t dst_start, size_t n)
{
    if (!Chunk_bounds_check(src, src_start, n)) return;
    if (!Chunk_bounds_check(dst, dst_start, n)) return;

    // copy the bytes
    memcpy(dst->data + dst_start, src->data + src_start, n);
}
";

    assert_merge!(original, a, b, Ok(expected_diffy), "Myers diffy merge");
}

#[test]
fn correct_range_is_used_for_both_case() {
    let base = r#"
class GithubCall(db.Model):

`url`: URL of request Example.`https://api.github.com`
"#;

    let theirs = r#"
class GithubCall(db.Model):

`repo`: String field. Github repository fields. Example: `amitu/python`
"#;

    let ours = r#"
class Call(models.Model):
`body`: String field. The payload of the webhook call from the github.

`repo`: String field. Github repository fields. Example: `amitu/python`
"#;

    let expected = r#"
class Call(models.Model):
`body`: String field. The payload of the webhook call from the github.

`repo`: String field. Github repository fields. Example: `amitu/python`
"#;

    assert_merge!(base, ours, theirs, Ok(expected), "MergeRange::Both case");
}

#[test]
fn delete_and_insert_conflict() {
    let base = r#"
{
    int a = 2;
}
"#;

    let ours = r#"
{
}
"#;

    let theirs = r#"
{
    int a = 2;
    int b = 3;
}
"#;

    let expected = r#"
{
<<<<<<< ours
||||||| original
    int a = 2;
=======
    int a = 2;
    int b = 3;
>>>>>>> theirs
}
"#;

    assert_merge!(
        base,
        ours,
        theirs,
        Err(expected),
        "MergeRange (Ours::delete, Theirs::insert) conflict"
    );

    let expected = r#"
{
<<<<<<< ours
    int a = 2;
    int b = 3;
||||||| original
    int a = 2;
=======
>>>>>>> theirs
}
"#;

    assert_merge!(
        base,
        theirs,
        ours,
        Err(expected),
        "MergeRange (Theirs::delete, Ours::insert) conflict"
    );
}

#[test]
fn conflict_hunks_without_trailing_newline_glue_markers_by_default() {
    let base = "This is line 1.\nThis is line 2.";
    let ours = "This is line 1.\nThis is line 2 changed.";
    let theirs = "This is line 1.\nThis is line 2 also changed.";

    // The default IncompleteHunkStyle::Diff3 matches GNU `diff3 -m`, which
    // appends the succeeding markers directly to incomplete lines.
    let expected = "\
This is line 1.
<<<<<<< ours
This is line 2 changed.||||||| original
This is line 2.=======
This is line 2 also changed.>>>>>>> theirs
";

    assert_merge!(
        base,
        ours,
        theirs,
        Err(expected),
        "file-final hunks without trailing newline",
    );
}

#[test]
fn conflict_hunks_without_trailing_newline_git_style_keeps_markers_on_own_lines() {
    let base = "This is line 1.\nThis is line 2.";
    let ours = "This is line 1.\nThis is line 2 changed.";
    let theirs = "This is line 1.\nThis is line 2 also changed.";

    // IncompleteHunkStyle::Git matches `git merge-file --diff3`, which inserts
    // a newline after an incomplete line so that every marker starts a line.
    let expected = "\
This is line 1.
<<<<<<< ours
This is line 2 changed.
||||||| original
This is line 2.
=======
This is line 2 also changed.
>>>>>>> theirs
";

    let mut options = MergeOptions::new();
    options.set_incomplete_hunk_style(IncompleteHunkStyle::Git);

    assert_eq!(
        options.merge(base, ours, theirs),
        Err(String::from(expected)),
        "file-final hunks without trailing newline with the git style",
    );
    assert_eq!(
        options.merge_bytes(base.as_bytes(), ours.as_bytes(), theirs.as_bytes()),
        Err(expected.as_bytes().to_vec()),
        "file-final hunks without trailing newline with the git style (bytes)",
    );
}

#[test]
fn conflict_hunk_trailing_newline_permutations() {
    const BASE: &str = "This is line 1.\nThis is line 2.";
    const BASE_NL: &str = "This is line 1.\nThis is line 2.\n";
    const OURS: &str = "This is line 1.\nThis is line 2 changed.";
    const OURS_NL: &str = "This is line 1.\nThis is line 2 changed.\n";
    const THEIRS: &str = "This is line 1.\nThis is line 2 also changed.";
    const THEIRS_NL: &str = "This is line 1.\nThis is line 2 also changed.\n";

    // Each case lists the inputs along with the expected output of the
    // default IncompleteHunkStyle::Diff3. The expected outputs were verified
    // against GNU diff3 3.12 (`diff3 -m`), which glues the marker succeeding
    // each incomplete hunk onto the hunk's final line.
    let cases = [
        (
            BASE,
            OURS,
            THEIRS,
            "\
This is line 1.
<<<<<<< ours
This is line 2 changed.||||||| original
This is line 2.=======
This is line 2 also changed.>>>>>>> theirs
",
        ),
        (
            BASE,
            OURS,
            THEIRS_NL,
            "\
This is line 1.
<<<<<<< ours
This is line 2 changed.||||||| original
This is line 2.=======
This is line 2 also changed.
>>>>>>> theirs
",
        ),
        (
            BASE,
            OURS_NL,
            THEIRS,
            "\
This is line 1.
<<<<<<< ours
This is line 2 changed.
||||||| original
This is line 2.=======
This is line 2 also changed.>>>>>>> theirs
",
        ),
        (
            BASE,
            OURS_NL,
            THEIRS_NL,
            "\
This is line 1.
<<<<<<< ours
This is line 2 changed.
||||||| original
This is line 2.=======
This is line 2 also changed.
>>>>>>> theirs
",
        ),
        (
            BASE_NL,
            OURS,
            THEIRS,
            "\
This is line 1.
<<<<<<< ours
This is line 2 changed.||||||| original
This is line 2.
=======
This is line 2 also changed.>>>>>>> theirs
",
        ),
        (
            BASE_NL,
            OURS,
            THEIRS_NL,
            "\
This is line 1.
<<<<<<< ours
This is line 2 changed.||||||| original
This is line 2.
=======
This is line 2 also changed.
>>>>>>> theirs
",
        ),
        (
            BASE_NL,
            OURS_NL,
            THEIRS,
            "\
This is line 1.
<<<<<<< ours
This is line 2 changed.
||||||| original
This is line 2.
=======
This is line 2 also changed.>>>>>>> theirs
",
        ),
        (
            BASE_NL,
            OURS_NL,
            THEIRS_NL,
            "\
This is line 1.
<<<<<<< ours
This is line 2 changed.
||||||| original
This is line 2.
=======
This is line 2 also changed.
>>>>>>> theirs
",
        ),
    ];

    // `git merge-file --diff3` (verified with git 2.55.0) appends a newline to
    // every incomplete hunk, so all of the permutations render identically
    // with IncompleteHunkStyle::Git.
    let expected_git = "\
This is line 1.
<<<<<<< ours
This is line 2 changed.
||||||| original
This is line 2.
=======
This is line 2 also changed.
>>>>>>> theirs
";

    let diff3_options = MergeOptions::new();
    let mut git_options = MergeOptions::new();
    git_options.set_incomplete_hunk_style(IncompleteHunkStyle::Git);

    for (base, ours, theirs, expected_diff3) in cases {
        assert_eq!(
            diff3_options.merge(base, ours, theirs),
            Err(String::from(expected_diff3)),
            "diff3 style: base={base:?} ours={ours:?} theirs={theirs:?}",
        );
        assert_eq!(
            diff3_options.merge_bytes(base.as_bytes(), ours.as_bytes(), theirs.as_bytes()),
            Err(expected_diff3.as_bytes().to_vec()),
            "diff3 style (bytes): base={base:?} ours={ours:?} theirs={theirs:?}",
        );
        assert_eq!(
            git_options.merge(base, ours, theirs),
            Err(String::from(expected_git)),
            "git style: base={base:?} ours={ours:?} theirs={theirs:?}",
        );
        assert_eq!(
            git_options.merge_bytes(base.as_bytes(), ours.as_bytes(), theirs.as_bytes()),
            Err(expected_git.as_bytes().to_vec()),
            "git style (bytes): base={base:?} ours={ours:?} theirs={theirs:?}",
        );
    }
}
