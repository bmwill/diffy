#![no_main]

use diffy::patch_set::{ParseOptions, PatchSet};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Consume every item to avoid short-circuiting on first `Err`.
    for result in PatchSet::parse_bytes(data, ParseOptions::unidiff()) {
        let _ = result;
    }
});
