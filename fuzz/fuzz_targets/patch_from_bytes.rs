#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Should never panic - only return Ok or Err
    let _ = diffy::Patch::from_bytes(data);
});
