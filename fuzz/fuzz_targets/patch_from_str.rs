#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Should never panic - only return Ok or Err
    let _ = diffy::Patch::from_str(data);
});
