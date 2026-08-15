# tox-ffi

Raw FFI bindings to [c-toxcore](https://github.com/TokTok/c-toxcore).

## Structure

- `src/lib.rs` — **curated hand-written subset** (the code that is actually
  linked). Uses the ABI-safe `tox_options_set_*` accessors instead of struct
  layout, so it stays compatible across c-toxcore 0.2.x releases.
- `src/bindgen_output.rs` — full **bindgen 0.72.1** output (checked in,
  verification artifact only, not compiled). Regenerate command is in the
  file header. Cross-checked: all signatures in `lib.rs` match.

## Linking

`build.rs` locates the static `toxcore.lib`:

1. `TOXCORE_LIB` env var → directory containing `toxcore.lib`
2. default: `<workspace>/third_party/c-toxcore/build`

libsodium: pass `SODIUM_LIB` env var pointing at the directory containing
`sodium.lib` (e.g. `C:\Users\<you>\vcpkg\installed\x64-windows\lib`), or use
`vcpkg integrate install` so the search path is global.

Windows system libs (`ws2_32`, `iphlpapi`, `advapi32`, `user32`, `shell32`,
`bcrypt`) are linked automatically.
