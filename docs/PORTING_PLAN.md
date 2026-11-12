# EaglerRust 26.3 Porting Plan

## Goal

Build a genuine Rust + WebAssembly reimplementation targeting Eaglercraft 26.3. The 26.1.2 source archive is a reference for understanding behavior and algorithms; it is not the target runtime and will not be embedded as a Java/TeaVM client.

## Source reference: Eaglercraft 26.1.2

Preliminary archive inventory (not yet a full source audit):

- 1,893 ZIP entries
- 961 `.java` files
- 896 Java files are under `sources/teavm`
- 25 Java files are under `patch-compile/java`
- 0 Rust source files
- The archive also contains TypeScript, documentation, build scripts, assets, and compiled artifacts.

Important: the Java count includes more than the game client. It includes TeaVM compatibility code and other components. We must identify actual client packages before estimating porting scope. The archive README, architecture docs, package inventory, and build process still need a complete review.

## Target architecture

- **Rust core:** game state, world/chunk data, entities, physics, input abstraction, protocol, and other portable logic.
- **WebAssembly:** compile the Rust client core for `wasm32-unknown-unknown`.
- **Browser host:** minimal HTML/CSS and narrowly scoped JavaScript glue for browser APIs not directly exposed by the Rust bindings.
- **Rendering:** Rust-owned rendering logic using browser WebGL APIs, introduced after the core data model is established.
- **Multiplayer:** Rust WebSocket/protocol implementation compatible with the intended Eaglercraft 26.3 servers.
- **Deployment:** GitHub Actions + GitHub Pages.

The existing EaglerXPaper server may be used as a compatibility target for multiplayer testing, but it is server-side software and does not supply the browser client.

## Porting workflow

### Phase 0 — Source and version audit

1. Read the 26.1.2 README, architecture, build, protocol, and migration documentation.
2. Inventory Java packages and classify files as client code, TeaVM patches, gateway/server code, tests, or generated/decompiled references.
3. Identify the exact upstream/source revision and licensing obligations.
4. Compare 26.1.2 behavior and protocol definitions with the 26.3 target HTML/client.
5. Record confirmed differences separately from assumptions.

**Exit criteria:** a package-level source map and a version-difference checklist.

### Phase 1 — Client subsystem map

Map the client into independently testable subsystems:

- startup and lifecycle
- settings and persistence
- input, pointer lock, and fullscreen
- world and chunk representation
- block/state registries and assets
- entities and simulation
- physics and collision
- rendering and shaders
- audio
- multiplayer connection, handshake, packet framing, and login

Do not assume every subsystem is available in the source archive; mark missing or uncertain areas.

### Phase 2 — Rust foundations

Create Rust crates/modules with clear boundaries. Start with portable data structures and deterministic tests before browser integration. Keep protocol parsing separate from browser WebSocket transport.

### Phase 3 — Vertical slices

Implement and test small end-to-end slices rather than attempting a full rewrite at once. Suggested order:

1. buildable WASM shell and diagnostics
2. core registries/data structures
3. chunk decoding and world representation
4. input and basic camera
5. first rendered scene (initial hard-coded voxel terrain prototype now exists)
6. entity updates and physics
7. protocol handshake and multiplayer
8. assets, audio, persistence, and remaining gameplay

The sequence may change after the source audit reveals dependencies.

### Phase 4 — 26.3 parity and release

Maintain a compatibility checklist against the 26.3 target. Test browser builds, performance, persistence, single-player behavior, and multiplayer compatibility before calling the client playable.

## Engineering rules

- Do not copy the old compiled JavaScript/WASM runtime and call it a Rust port.
- Do not treat server/gateway code as client implementation.
- Preserve upstream copyright notices and comply with the source archive's license when reusing code or assets.
- Keep a clear distinction between confirmed source facts, reverse-engineered observations, and implementation assumptions.
- Keep the repository buildable at every milestone.

## Current status

The repository boots a Rust-owned WebGL1 renderer and connects browser input to a Rust player controller. Its current movement slice translates the 26.1.2 reference constants for walking/sprinting acceleration, jump impulse, gravity, input scaling, and horizontal/vertical drag. Collision uses full-cube AABBs and Y-X-Z axis resolution. The camera follows the player. This is a reference-informed baseline only; exact 26.3 behavior has not yet been audited. Browser raycasting targets blocks from the crosshair and draws a 3D outline around the selected block. Desktop left-click and mobile BREAK actions remove the targeted block; desktop right-click and mobile PLACE actions place the selected hotbar item. Successful edits rebuild the current world mesh and upload it to the WebGL buffer. The nine-slot hotbar is visible in the browser and selectable by touch/mouse or number keys 1–9. This is an integrated local gameplay prototype, not full 26.3 parity. The renderer now stores a separate GPU buffer per section and rebuilds only dirty section meshes after edits. A radius-one chunk manager maintains a 3×3 target area of 16×16 columns around the player, loading and unloading columns as the player crosses chunk boundaries and releasing buffers for unloaded sections. During runtime, streaming is budgeted to one column load and one unload per fixed 20 Hz tick; missing columns are selected by distance with deterministic tie-breaking, and pending load/unload counts are tracked. Initial startup still fills the initial 3×3 area before rendering. Generation remains synchronous and small-radius only; worker-based generation and 26.3 chunk decoding are not implemented. Official 26.3 block/item registry mapping, texture atlases and render layers, asynchronous chunk jobs, complex collision shapes, biome/cave generation, entities, browser persistence, multiplayer, and exact 26.3 movement parity remain future work. The 26.1.2 archive still has only a preliminary file-count inventory; its documentation and source packages have not yet been fully audited. Continue the Phase 0 source map alongside gameplay and renderer validation.
