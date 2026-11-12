# EaglerRust 26.3

A Rust + WebAssembly client reimplementation project targeting modern browsers.

## Current status

The browser shell boots a Rust-owned WebGL1 renderer. Rust stores block states in 16×16×16 chunk sections and has a sparse world store that reads and writes blocks by world coordinates, including negative coordinates and section boundaries. An internal deterministic terrain generator fills a 48×48 prototype region with placeholder grass, dirt, stone, water, sand, logs, and leaves. The renderer uses temporary block-state material palettes with directional face shading. It generates a 48×48 terrain region with more detailed height variation, sandy shorelines, shallow water, and deterministic tree trunks and foliage. It iterates all loaded sections, combines their exposed-face meshes, and uploads the result to a GPU buffer for perspective rendering.

This is still a renderer prototype, not a playable Minecraft client. The displayed sample section is populated procedurally inside Rust; large-scale terrain streaming, biome and cave systems, GPU mesh rebuilds after edits, the real 26.3 block registry mapping, texture atlases, collision-aware player physics, block interaction, entities, and multiplayer are not implemented yet. The current palette colours are placeholders, not official Minecraft textures.

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

## Massive Game Update — world generation and navigation

The renderer uses a Rust-owned free-fly camera instead of an automatic orbit. WASD moves horizontally, Space and Shift move vertically, Control moves faster, and pointer dragging rotates the view. Desktop mouse input requests pointer lock; touch input uses pointer dragging. The generated landscape now spans 48×48 blocks and includes height detail, water, sand, logs, and leaves. This remains an early prototype: collision, gravity, grounded jumping, block interaction, and player animation are still future work.

## Next milestones

1. Connect dirty-section tracking to GPU mesh uploads and implement chunk loading/unloading policy.
2. Map placeholder block IDs to the actual 26.3 block registry.
3. Add block textures, atlas sampling, lighting, and fog.
4. Add collision, gravity, grounded jumping, and a proper player controller.
5. Add world interaction, entities, and multiplayer compatibility.

A compiled TeaVM/WASM artifact is not a substitute for the original Java source. The project will keep distinguishing implemented Rust systems from compatibility work that is still planned.
