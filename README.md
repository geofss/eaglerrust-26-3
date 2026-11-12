# EaglerRust 26.3

A Rust + WebAssembly client reimplementation project targeting modern browsers.

## Current status

The browser shell boots a Rust-owned WebGL1 renderer. Rust stores block states in 16×16×16 chunk sections and has a sparse world store that reads and writes blocks by world coordinates, including negative coordinates and section boundaries. An internal deterministic heightmap generator now fills a 16×16 prototype region with placeholder grass, dirt, and stone states. The renderer uses temporary block-state material palettes with directional face shading. It generates a 32×32 terrain region, iterates all loaded sections, combines their exposed-face meshes, and uploads the result to a GPU buffer for perspective rendering.

This is still a renderer prototype, not a playable Minecraft client. The displayed sample section is populated procedurally inside Rust; large-scale terrain streaming, biome and cave systems, GPU mesh rebuilds after edits, the real 26.3 block registry mapping, texture atlases, player controls, world interaction, entities, and multiplayer are not implemented yet. The current palette colours are placeholders, not official Minecraft textures.

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

1. Verify the generated terrain renderer on desktop and mobile browsers.
2. Connect dirty-section tracking to GPU mesh uploads and implement chunk loading/unloading policy, using deterministic loaded-section iteration.
3. Expand terrain generation and map placeholder block IDs to the actual 26.3 block registry.
4. Add block textures, atlas sampling, lighting, and fog.
4. Add player camera controls and collision-aware movement.
5. Port the remaining client systems and validate 26.3 compatibility.

A compiled TeaVM/WASM artifact is not a substitute for the original Java source. The project will keep distinguishing implemented Rust systems from compatibility work that is still planned.
