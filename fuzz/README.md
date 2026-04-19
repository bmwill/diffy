# Fuzzing

Uses [cargo-fuzz] with libFuzzer.

[cargo-fuzz]: https://github.com/rust-fuzz/cargo-fuzz

## Setup

```bash
cargo +nightly install cargo-fuzz
```

## Run

```bash
# List targets
cargo +nightly fuzz list

# Run specific target (indefinitely)
cargo +nightly fuzz run patch_from_str

# Run with time limit (seconds)
cargo +nightly fuzz run patch_from_str -- -max_total_time=60

# Run all targets (quick smoke test)
for t in $(cargo +nightly fuzz list); do
  cargo +nightly fuzz run $t -- -max_total_time=10
done
```

## Targets

| Target                    | Tests                                   |
|---------------------------|-----------------------------------------|
| `patch_from_str`          | `Patch::from_str()`                     |
| `patch_from_bytes`        | `Patch::from_bytes()`                   |
| `patch_set_gitdiff`       | `PatchSet::parse(..., gitdiff())`       |
| `patch_set_unidiff`       | `PatchSet::parse(..., unidiff())`       |
| `patch_set_gitdiff_bytes` | `PatchSet::parse_bytes(..., gitdiff())` |
| `patch_set_unidiff_bytes` | `PatchSet::parse_bytes(..., unidiff())` |

## Crashes

Crash inputs are saved to `fuzz/artifacts/<target>/`.
To reproduce:

```bash
cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-<hash>
```
