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

The repository now boots a Rust-owned WebGL1 renderer and draws a small hard-coded voxel terrain mesh with shaders, depth testing, perspective projection, and an orbiting camera. This is a graphics prototype, not a playable client: chunk streaming, block registries, textures, player controls, world interaction, entities, and multiplayer remain unimplemented. The 26.1.2 archive has had a preliminary file-count inventory, but its documentation and source packages have not yet been fully audited. Continue the Phase 0 source map alongside renderer validation and chunk-backed world integration.
