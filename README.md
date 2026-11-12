# EaglerRust 26.3

A Rust + WebAssembly client reimplementation project targeting modern browsers.

## Current status

The browser shell boots a Rust-owned WebGL1 renderer. Rust stores block states in 16×16×16 chunk sections and has a sparse world store that reads and writes blocks by world coordinates, including negative coordinates and section boundaries. An internal deterministic terrain generator fills a 48×48 prototype region with placeholder grass, dirt, stone, water, sand, logs, and leaves. The renderer uses temporary block-state material palettes with directional face shading. It generates a 48×48 terrain region with more detailed height variation, sandy shorelines, shallow water, and deterministic tree trunks and foliage. It iterates all loaded sections, combines their exposed-face meshes, and uploads the result to a GPU buffer for perspective rendering.

The browser now drives a vanilla-inspired Rust player controller. Its movement baseline uses audited 26.1.2 reference constants: 0.1 walking speed, 0.13 sprint speed, 0.42 jump impulse, 0.08 gravity, 0.98 input/vertical drag, 0.91 horizontal drag, and the 0.21600002 ground-acceleration factor. Full-cube collision resolves movement in Y-X-Z order. These are 26.1.2 reference behaviors, not yet verified against the 26.3 target. The camera follows the player, and the browser can raycast from the crosshair, break the targeted block, or place the selected hotbar block. A radius-one chunk manager maintains a 3×3 target area of 16×16 columns around the player. Runtime streaming is budgeted to one column load and one column unload per 20 Hz simulation tick, prioritizes nearby missing columns deterministically, and reports pending work; initial startup still fills the initial 3×3 area before rendering. Each section has its own GPU buffer; edits rebuild only dirty section meshes and update only their buffers, while removed columns release their buffers. The nine-slot hotbar is exposed through wasm-bindgen and rendered as a browser HUD; mouse/touch selection and number keys 1–9 update the Rust inventory model. Desktop controls use left-click to break and right-click to place; touch controls expose jump, sprint, break, and place actions. This is an integrated local gameplay prototype, not full 26.3 gameplay parity. The movement slice still lacks block-specific friction, crouching, step-up collision, fluids, effects, vehicles, and creative flight. Chunk generation is currently synchronous but spread across fixed simulation ticks during travel and limited to a small radius; block IDs and material colours remain placeholders. The real 26.3 registry, texture atlases, asynchronous generation, biomes, caves, entities, multiplayer, and exact movement rules are still pending. A deterministic, versioned binary snapshot codec now round-trips the internal world and validates signatures, versions, section counts, duplicate coordinates, and payload lengths; it is not yet connected to browser localStorage or IndexedDB.

## Architecture

- **Rust + wasm-bindgen:** mesh generation, camera math, renderer setup, and frame submission.
- **WebAssembly + WebGL1:** browser GPU rendering through web-sys bindings.
- **HTML/CSS/JavaScript:** page shell, WASM bootstrap, keyboard/pointer/touch controls, hotbar HUD, block-action events, resize handling, and animation-frame scheduling.
- **GitHub Actions:** native tests, WASM build, and GitHub Pages deployment.

## Build locally

Install Rust, the `wasm32-unknown-unknown` target, and `wasm-pack`, then run:

```sh
cd crates/client
wasm-pack build --target web --release --out-dir ../../web/pkg
```

Serve the `web/` directory over HTTP; opening `index.html` directly with `file://` will not work because browsers restrict WASM module loading.

## Massive Game Update — world generation and navigation

The Rust player controller now drives movement and the camera follows the player. WASD moves, Space jumps, Control or Shift sprints, and pointer dragging rotates the view. Desktop left-click breaks the targeted block and right-click places the selected hotbar block. Mobile controls expose movement, jump, sprint, break, and place. World edits rebuild the prototype mesh and upload it to WebGL. The generated landscape spans 48×48 blocks and includes height detail, water, sand, logs, and leaves.

## Next milestones

1. Replace placeholder block IDs with the audited 26.3 block/state registry.
2. Add texture atlas sampling, transparent/cutout render passes, lighting, and fog.
3. Move chunk generation off the frame-critical path into worker-based jobs; the current per-tick work budget is a synchronous stepping stone, not asynchronous generation.
4. Add biomes, caves, height limits, and 26.3-compatible chunk decoding.
5. Add entities, persistence, audio, and the audited multiplayer handshake/protocol.

A compiled TeaVM/WASM artifact is not a substitute for the original Java source. The project will keep distinguishing implemented Rust systems from compatibility work that is still planned.
