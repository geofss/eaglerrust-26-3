# EaglerRust 26.3

A Rust + WebAssembly client reimplementation project targeting modern browsers.

## Current status

The browser shell boots a Rust-owned WebGL1 renderer. Rust stores block states in 16×16×16 chunk sections and has a sparse world store that reads and writes blocks by world coordinates, including negative coordinates and section boundaries. An internal deterministic terrain generator fills a 48×48 prototype region with placeholder grass, dirt, stone, water, sand, logs, and leaves. The renderer uses temporary block-state material palettes with directional face shading. It generates a 48×48 terrain region with more detailed height variation, sandy shorelines, shallow water, and deterministic tree trunks and foliage. It iterates all loaded sections, combines their exposed-face meshes, and uploads the result to a GPU buffer for perspective rendering.

The browser now drives a vanilla-inspired Rust player controller. Its movement baseline uses audited 26.1.2 reference constants: 0.1 walking speed, 0.13 sprint speed, 0.42 jump impulse, 0.08 gravity, 0.98 input/vertical drag, 0.91 horizontal drag, and the 0.21600002 ground-acceleration factor. Full-cube collision resolves movement in Y-X-Z order. These are 26.1.2 reference behaviors, not yet verified against the 26.3 target. The camera follows the player, and the browser can raycast from the crosshair, break the targeted block, or place the selected hotbar block. A radius-one chunk manager maintains a 3×3 target area of 16×16 columns around the player. Runtime streaming is budgeted to one column load and one column unload per 20 Hz simulation tick, prioritizes nearby missing columns deterministically, and reports pending work; initial startup still fills the initial 3×3 area before rendering. Each section has its own GPU buffer; edits rebuild only dirty section meshes and update only their buffers, while removed columns release their buffers. The nine-slot hotbar is exposed through wasm-bindgen and rendered as a browser HUD; mouse/touch selection and number keys 1–9 update the Rust inventory model. Desktop controls use hold-left-click to mine and right-click to place; touch controls expose jump, sprint, hold-to-mine, and place actions. This is an integrated local gameplay prototype, not full 26.3 gameplay parity. The movement slice still lacks block-specific friction, crouching, step-up collision, fluids, effects, vehicles, and creative flight. Chunk generation is currently synchronous but spread across fixed simulation ticks during travel and limited to a small radius; block IDs and material colours remain placeholders. The five new porting foundations are now present but deliberately scoped: namespaced metadata for the small prototype block registry; a byte-per-voxel palette decoder isolated for the supplied 26.1.2 reference gateway format; CPU atlas layout plus generated development pixels uploaded to WebGL, per-face UV mesh data, and fragment-shader sampling. These are placeholders, not extracted Eaglercraft assets. A prototype lower-slab collision shape and bounded VarInt packet framing are also present. None of these alone establishes 26.3 compatibility. The full 26.3 registry, verified 26.3 chunk codec, resource-pack extraction, transparent/cutout passes, complete voxel-shape/step-up physics, and complete gameplay packet schemas remain pending. A browser WebSocket lifecycle and a separate 26.1.2-derived reference protocol path now exist; they are not evidence of 26.3 compatibility. The protocol module also has an isolated vanilla Java Edition 26.3 handshake encoder using protocol 777, with validation tests. It is deliberately not wired into the live Eagler connection yet: The browser transport now supports configurable WebSocket subprotocol negotiation and reports the selected protocol and negotiated extensions; the exact 26.3 bootstrap and accepted protocol variants still require source-level verification. The 26.3 login/configuration packet layouts also remain unverified. A deterministic, versioned binary world snapshot codec validates signatures, versions, section counts, duplicate coordinates, and payload lengths. It now sits inside a versioned singleplayer save container that also stores the world seed and player location, with browser localStorage wired to the save/load controls.

## Local singleplayer integrated-server foundation

The local world path now has a Rust-owned integrated-server core alongside the experimental Eagler V5 connector. The local server owns the world snapshot, seed, player state, game mode, world/day clock, and chunk manager. It advances at a fixed 20 ticks per second, streams chunks around the player with bounded work per tick, validates authoritative local block edits against loaded chunks/world bounds/game mode, and requests an autosave every 600 ticks (30 seconds).

The browser UI supports creating a seeded world, selecting Survival/Creative/Adventure/Spectator mode, manual save/load, and timed autosave to browser localStorage. Save format v2 stores game mode and reads earlier v1 saves as Creative. The renderer mirrors the server-owned world for mesh generation; multiplayer continues to use its separate network path.

This is a meaningful local integrated-server prototype, **not yet the original Eaglercraft integrated server or a complete Minecraft 26.3 singleplayer port**. The current server core runs on the browser main thread; it does not yet have the source project's dedicated Web Worker + IPC isolation. The world generator and block registry are still project-local prototypes. Missing systems include the verified 26.3 registry and resource pack, Minecraft-compatible region/level/player storage, full biomes/caves/structures, lighting, fluids, entities and AI, survival hunger/health/drop rules, crafting, block entities, redstone, dimensions, commands, and full original-game parity. Browser localStorage is subject to quota and site-data clearing; export/import and IndexedDB-backed world management remain future work.

## Architecture

- **Rust + wasm-bindgen:** mesh generation, camera math, renderer setup, and frame submission.
- **WebAssembly + WebGL1:** browser GPU rendering through web-sys bindings.
- **HTML/CSS/JavaScript:** page shell, WASM bootstrap, keyboard/pointer/touch controls, hotbar HUD, block-action events, resize handling, and animation-frame scheduling.
- **GitHub Actions:** native workspace tests, browser WASM target validation, and a generated wasm-pack browser package build.

## Build locally

Install Rust, the `wasm32-unknown-unknown` target, and `wasm-pack`, then run:

```sh
cd crates/client
wasm-pack build --target web --release --out-dir ../../web/pkg
```

Serve the `web/` directory over HTTP; opening `index.html` directly with `file://` will not work because browsers restrict WASM module loading.

## Massive Game Update — world generation and navigation

The Rust player controller now drives movement and the camera follows the player. WASD moves, Space jumps, Control or Shift sprints, and pointer dragging rotates the view. Desktop left-click breaks the targeted block and right-click places the selected hotbar block. Mobile controls expose movement, jump, sprint, break, and place. World edits rebuild the prototype mesh and upload it to WebGL. The generated landscape spans 48×48 blocks and includes height detail, water, sand, logs, and leaves.

## Porting foundations added together

- **Registry:** namespaced identifiers and collision metadata are now represented in the prototype registry. Its numeric IDs and nine entries are not the complete 26.3 registry.
- **Chunk codec:** a validated decoder handles the 4096-byte palette-index layout documented by the supplied 26.1.2 reference gateway. This is intentionally separate from the 26.3 network path and is not the vanilla packed-long palette format.
- **Texture atlas:** normalized layout, deterministic development pixels, WebGL upload, per-face UV mesh data, and fragment-shader sampling are integrated. These are placeholders rather than extracted Eaglercraft textures; verified resource-pack mapping and transparent/cutout passes remain pending.
- **Collision:** the prototype physics path can resolve full cubes and one internal lower-slab shape. Stair/partial voxel shapes, fluids, step-up retry, and the full target-version shape registry remain future work.
- **Networking:** bounded browser WebSocket transport, a login/configuration state machine, and a live packet pump are implemented for the supplied 26.1.2 reference protocol 775. Reference play support currently includes login, chunk columns, block updates, chunk unloads, teleports, hotbar slot updates, movement, digging, and use-item-on. Compression, encryption, full registries, full packet coverage, and 26.3 verification are not implemented.

## Next milestones

1. Verify the exact 26.3 registry and chunk payload format from the target runtime before mapping IDs or accepting packets.
2. Connect atlas pixels, per-face UVs, transparent/cutout passes, lighting, and fog to the WebGL renderer.
3. Expand collision shapes and implement step-up behavior, then compare movement traces with the target.
4. Compare the reference protocol path against the 26.3 runtime and replace each mismatched packet/registry/chunk assumption with verified target behavior.
5. Move chunk work to workers and add biomes, caves, entities, browser persistence, and audio.

A compiled TeaVM/WASM artifact is not a substitute for the original Java source. The project will keep distinguishing implemented Rust systems from compatibility work that is still planned.


## Reference section ingestion integration

The 26.1.2 reference byte-per-voxel palette decoder now has an atomic adapter into the internal world store. A complete section is validated and decoded before it replaces the target section; malformed palette payloads leave the world untouched. Installing a section marks its own mesh and any already-loaded face-neighbour sections dirty so the renderer's existing dirty-section rebuild path can refresh exposed boundaries. This remains a reference-format integration test path, not a verified 26.3 server packet decoder.


## Textured WebGL prototype pipeline

The renderer generates a deterministic RGBA development atlas, uploads it as a WebGL texture, stores UV coordinates per mesh vertex, and samples the atlas in the fragment shader. Grass selects separate top, side, and bottom tiles; other current prototype blocks use dedicated placeholder tiles. Nearest filtering preserves crisp pixel edges. The selection outline uses a dedicated white tile. These generated pixels are not extracted Eaglercraft assets and do not establish target-version texture mappings or transparency behavior.


## Live reference server path (26.1.2-derived)

The browser page now exposes an experimental WebSocket connection panel. The Rust connection driver pumps complete binary WebSocket messages, advances the reference login/configuration state machine, and forwards play packets to the renderer. The current reference handlers cover Play Login, chunk columns, block updates, chunk unloads, player teleports (including teleport confirmation), container-slot updates for the hotbar, player movement, hotbar selection, dig start/abort/finish, and use-item-on.

The browser normalizes bare hosts and `http(s)://` or `ws://` inputs to secure `wss://` URLs. Standard EaglerXServer connections do not request an HTTP WebSocket subprotocol by default: the Eagler V5 application handshake is sent as the first binary WebSocket message. An optional HTTP subprotocol can be supplied for a custom gateway. Negotiated protocol and extensions are captured on the browser WebSocket `open` event; WebSocket framing and negotiated permessage-deflate remain browser-managed.

The 26.1.2 source uses a minimum overworld height of -64 and chunk section indices relative to that minimum; the adapter converts those section indices to the Rust world's absolute section coordinates. A small set of source-verified reference block IDs is translated into the prototype render/collision definitions. Unknown block states are preserved distinctly and use a fallback tile.

## Hold-to-mine lifecycle integration

The renderer now drives mining while the desktop mouse or mobile BREAK control is held. Local mode advances the prototype hardness timer and removes a block only when the action completes. In the 26.1.2-derived reference-server mode, the frame loop queues Player Action start, abort, and finish events against the captured block position and face; the browser network pump sends those events, and server block updates remain authoritative. Completed targets are latched to avoid repeatedly restarting a finished dig while the server update is in flight. Login resets any stale mining state. Unit tests cover the reference dig payload status encoding and invalid status rejection.

The mining timer still uses the small prototype hardness registry. It does not yet model tools, game mode, effects, server-side mining speed, or the complete 26.3 packet definitions. The reference status values and packet ID remain 26.1.2-derived assumptions until checked against a real 26.3-compatible endpoint.

**This is not yet a playable Eaglercraft 26.3 client.** The connection panel is only suitable for a server that speaks the exact reference gateway protocol. The project still needs a real end-to-end server test, full target-version packet/registry/chunk verification, original resource assets, broader gameplay systems, and runtime/browser validation.


## Eagler V5 transport status

The browser connector begins standard EaglerXServer sessions with the Eaglercraft V5
application-level handshake (01 02 client-version frame), negotiates Eagler protocol 5
and the Java 26.3 protocol baseline (777), performs the no-password login exchange, and
sends the vanilla Login Acknowledged packet after synthesizing Login Success from the
server-granted identity. This handshake is separate from the HTTP WebSocket subprotocol;
the default is to offer no HTTP subprotocol, as standard EaglerXServer does not require one.

The V5 login path currently offers no password, cookies, skin/cape profile data, or optional
capabilities. Configuration/play packet identifiers and gameplay packet coverage still need
independent 26.3 verification, and a real EaglerXServer join test is still required. A green
unit/wasm build does not establish end-to-end server compatibility.
