#![no_main]

use diffy::patch_set::{ParseOptions, PatchSet};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    for result in PatchSet::parse(data, ParseOptions::gitdiff()) {
        // Consume every item to avoid short-circuiting on first `Err`.
        let _ = result;
    }
});
