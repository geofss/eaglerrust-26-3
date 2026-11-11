# EaglerRust 26.3

A Rust + WebAssembly client reimplementation project targeting modern browsers.

## Current status

The browser shell now boots a Rust-owned WebGL1 renderer. Rust builds a small voxel terrain mesh, uploads it to a GPU buffer, compiles vertex/fragment shaders, and renders it with a perspective camera that slowly orbits the scene.

This is the first real 3D rendering slice, not a playable Minecraft client. The scene is a tiny hard-coded terrain test; chunk loading, block registries, textures, player controls, world interaction, entities, and multiplayer are not implemented yet.

## Architecture

- **Rust + wasm-bindgen:** mesh generation, camera math, renderer setup, and frame submission.
- **WebAssembly + WebGL1:** browser GPU rendering through web-sys bindings.
- **HTML/CSS/JavaScript:** page shell, WASM bootstrap, resize handling, and animation-frame scheduling.
- **GitHub Actions:** native tests, WASM build, and GitHub Pages deployment.

## Build locally

Install Rust, the `wasm32-unknown-unknown` target, and `wasm-pack`, then run:

```sh
cd crates/client
wasm-pack build --target web --release --out-dir ../../web/pkg
```

Serve the `web/` directory over HTTP; opening `index.html` directly with `file://` will not work because browsers restrict WASM module loading.

## Next milestones

1. Verify the voxel renderer on desktop and mobile browsers.
2. Replace the hard-coded test terrain with chunk-backed world data.
3. Add block textures, atlas sampling, lighting, and fog.
4. Add player camera controls and collision-aware movement.
5. Port the remaining client systems and validate 26.3 compatibility.

A compiled TeaVM/WASM artifact is not a substitute for the original Java source. The project will keep distinguishing implemented Rust systems from compatibility work that is still planned.
