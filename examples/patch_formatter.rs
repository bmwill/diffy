use diffy::PatchFormatter;
use diffy::create_patch;

fn main() {
    let original = "first line\nlast line";
    let modified = "first line\nmodified last line";

    let patch = create_patch(original, modified);

    println!("PatchFormatter::Default");
    println!("{patch}");

    let formatter = PatchFormatter::new().missing_newline_message(false);
    println!("{formatter:?}");
    println!("{}", formatter.fmt_patch(&patch));

    let formatter = PatchFormatter::new().with_color();
    println!("{formatter:?}");
    println!("{}", formatter.fmt_patch(&patch));

    let formatter = PatchFormatter::new()
        .with_color()
        .missing_newline_message(false);
    println!("{formatter:?}");
    println!("{}", formatter.fmt_patch(&patch));
}
