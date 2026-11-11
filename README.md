# EaglerRust 26.3

A Rust + WebAssembly client reimplementation project targeting modern browsers.

## Current status

The browser shell boots a Rust-owned WebGL1 renderer. Rust stores block states in a 16×16×16 chunk section, builds exposed-face voxel geometry from that storage, uploads the mesh to a GPU buffer, and renders it with a perspective camera that slowly orbits the scene. Adjacent blocks omit their shared internal face, with unit tests covering the mesh output.

This is still a renderer prototype, not a playable Minecraft client. The displayed sample section is populated procedurally inside Rust; real world generation, chunk streaming, block registries, textures, player controls, world interaction, entities, and multiplayer are not implemented yet.

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
2. Expand chunk-backed rendering into a real chunk/world pipeline, including section boundaries and chunk lifecycle.
3. Add block textures, atlas sampling, lighting, and fog.
4. Add player camera controls and collision-aware movement.
5. Port the remaining client systems and validate 26.3 compatibility.

A compiled TeaVM/WASM artifact is not a substitute for the original Java source. The project will keep distinguishing implemented Rust systems from compatibility work that is still planned.
