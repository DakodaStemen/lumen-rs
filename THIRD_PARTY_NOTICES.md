# Third-party notices

Lumen Rust is MIT licensed. Its dependencies retain their own licenses.
`Cargo.lock` records the exact dependency graph.

Core components include:

- mpv / libmpv: LGPL-2.1-or-later, or GPL, depending on how your distribution builds it.
  Lumen links it dynamically and does not bundle it — https://mpv.io
- libmpv2 (Rust bindings): LGPL-2.1 — https://github.com/kohsine/libmpv-rs
- egui/eframe: MIT OR Apache-2.0 — https://github.com/emilk/egui
- serde/serde_json: MIT OR Apache-2.0 — https://github.com/serde-rs
- sha2: MIT OR Apache-2.0 — https://github.com/RustCrypto/hashes
- fastrand: Apache-2.0 OR MIT — https://github.com/smol-rs/fastrand

Rust does not remove licensing obligations for bundled assets, fonts or third-party code.
