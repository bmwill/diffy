use crate::{
    patch::{Hunk, Line, Patch},
    utils::LineIter,
};

#[allow(dead_code)]
pub(crate) fn apply(pre_image: &str, patch: &Patch<'_>) -> String {
    let pre_image: Vec<_> = LineIter::new(pre_image).collect();
    let mut image = pre_image.clone();

    for hunk in patch.hunks() {
        apply_hunk(&pre_image, &mut image, hunk);
    }

    image.into_iter().collect()
}

fn apply_hunk<'a>(pre_image: &[&'a str], image: &mut Vec<&'a str>, hunk: &Hunk<'a>) {
    let mut pos1 = hunk.old_range().start().saturating_sub(1);
    let mut pos2 = hunk.new_range().start().saturating_sub(1);

    for line in hunk.lines() {
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
