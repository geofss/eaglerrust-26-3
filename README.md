# EaglerRust 26.3

A clean-room Rust + WebAssembly port project targeting modern browsers.

## Current status

This repository currently contains the build/deployment shell only. The original game runtime has **not** been ported yet. The supplied single-file HTML is a compiled distribution containing TeaVM-generated Java/WebAssembly code, JavaScript loader code, embedded binary payloads, and game resources; it is not the original Java source tree.

## Architecture

- **Rust + wasm-bindgen:** browser-side application and gameplay systems.
- **HTML/CSS/JavaScript:** minimal page shell and WASM bootstrap only.
- **GitHub Actions:** compile `wasm32-unknown-unknown` and publish the `web/` directory.
- **GitHub Pages:** static hosting for the browser build.

## Build locally

Install Rust, the `wasm32-unknown-unknown` target, and `wasm-pack`, then run:

```sh
cd crates/client
wasm-pack build --target web --release --out-dir ../../web/pkg
```

Serve the `web/` directory over HTTP; opening `index.html` directly with `file://` will not work because browsers restrict WASM module loading.

## Porting plan

1. Inventory the compiled HTML distribution and identify runtime boundaries, data formats, and browser APIs.
2. Recover or obtain the original Java source and build configuration where available.
3. Define Rust modules and stable interfaces for rendering, input, world/chunk data, networking, audio, and persistence.
4. Port and test one subsystem at a time against observable behavior.
5. Remove legacy TeaVM/game runtime dependencies only after equivalent Rust implementations pass compatibility tests.

A compiled TeaVM/WASM artifact is not a reliable substitute for the original Java source. This repository deliberately does not claim a complete rewrite until the systems are actually implemented and tested.
