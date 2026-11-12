# Source Audit and Porting Map

## Scope and version boundary

The target is the user-supplied compiled **Eaglercraft 26.3** browser distribution. The available source reference is **EaglercraftX 26.1.2**. The supplied 26.1.2 archive includes a TeaVM browser client and a documented reference gateway protocol. It is useful for understanding selected browser adapters and that reference implementation, but it is not proof of 26.3 parity or necessarily the official target protocol. Every ported behavior must be checked against the 26.3 target before being called compatible.

The 26.3 HTML is a compiled distribution, not an original source tree. It cannot be treated as a substitute for Java source. The 26.1.2 archive is therefore a reference, not the target runtime.

## Archive inventory

The supplied `eaglercraft-26.1.2-main.zip` was inspected:

- 1,893 top-level ZIP entries.
- 961 Java files across the top-level project tree.
- 411 Java files under `sources/teavm/java`.
- 25 Java compatibility/patch files under `patch-compile/java`.
- 21 Java files under `gateway/`.
- The archive contains `docs/decompiled-26.1.2-full-src.zip`, with 7,422 entries including 6,882 decompiled Java files.

The 6,882-file nested decompile is vanilla Minecraft reference material, not 6,882 Eaglercraft-specific implementation files. The 411-file TeaVM tree contains browser-port code, platform adapters, and selected Minecraft-facing overrides. The gateway code is server-side and must not be mistaken for the browser client.

## Reference map

| Rust subsystem | 26.1.2 reference locations | What to port or verify |
| --- | --- | --- |
| Block registry and state | `merged/net/minecraft/world/level/block/Blocks.java`, `Block.java`, `BlockBehaviour.java`, `BlockState.java` | Namespaced block IDs, state properties, render/collision behavior, runtime state registry. Numeric IDs must not be guessed. |
| World/chunks | `merged/net/minecraft/world/level/chunk/LevelChunkSection.java`, `PalettedContainer.java`, `ClientLevel.java` | Paletted section storage, chunk section bounds, biome/state decoding, load/unload lifecycle. |
| Section meshing | `merged/net/minecraft/client/renderer/chunk/SectionCompiler.java`, `FaceInfo.java`, `FaceBakery.java`, `ModelBlockRenderer.java`, `FluidRenderer.java` | Per-face geometry, model baking, UV projection, layer routing, fluids, and neighbor-aware rebuilds. |
| Player/input/physics | `merged/net/minecraft/client/player/LocalPlayer.java`, `KeyboardInput.java`, `merged/net/minecraft/world/entity/LivingEntity.java`, `Entity.java` | Tick ordering, acceleration, friction, jump, gravity, collision clipping, step-up, crouch, and camera interpolation. |
| Networking | `merged/net/minecraft/network/Connection.java`, `PacketBuffer.java`, protocol packet classes | Transport lifecycle, framing, handshake/configuration phases, packet codecs. Must be reconciled with 26.3 before implementing IDs. |
| Browser platform | `sources/teavm/java/net/lax1dude/eaglercraft/v2_6/internal/PlatformInput.java`, `PlatformNetworking.java`, `PlatformAudio.java`, `PlatformOpenGL.java` | Browser API boundaries and Eaglercraft-specific adapters. |
| Browser world/render adapters | `sources/teavm/java/net/lax1dude/eaglercraft/v2_6/adapter/WorldGlRenderer.java`, `WorldTextures.java`, `WorldStorage.java` | Eaglercraft-specific world rendering, texture loading, and storage behavior. |

Paths under `merged/` refer to entries inside the nested decompile ZIP. Paths under `sources/teavm/java/` refer to the top-level archive.

## Rust implementation audit

The current Rust tree has 15 modules. The key status is:

- `world.rs`, `chunk.rs`, `render.rs`: a real but deliberately small sparse-world, procedural terrain, chunk-streaming, and WebGL1 voxel prototype. `chunk_codec.rs` decodes only the documented 26.1.2 reference gateway's byte-per-voxel palette payload; `atlas.rs` computes CPU-side atlas tile/UV layout only.
- `block.rs`: a small namespaced prototype registry with nine entries and limited collision metadata; IDs and material behavior are not the official 26.3 registry.
- `player.rs`, `physics.rs`: a reference-informed movement baseline and full-cube AABB collision. Complex voxel shapes and step-up collision are not implemented.
- `raycast.rs`, `interaction.rs`, `inventory.rs`: local prototype block targeting, breaking/placing, and a nine-slot hotbar.
- `storage.rs`: an internal snapshot codec; no browser persistence adapter yet.
- `network.rs` and `packet.rs`: connection-state enum plus bounded VarInt and length-prefixed frame primitives; no WebSocket transport, handshake, or target packet schemas.
- `audio.rs`: audio lifecycle enum only; no Web Audio implementation.
- `app.rs`, `input.rs`: lifecycle and input primitives.

All 15 Rust source files were fetched and scanned for malformed literal newline/tab escape tokens; none were found in the current main branch. This source scan is not a substitute for compiling, tests, browser checks, or reviewing future diffs. The latest GitHub Actions run validates formatting, native unit tests, the WASM build, Pages artifact creation, and deployment.

## Recent reference-informed movement slice

The current movement implementation translates a small set of 26.1.2 reference constants into Rust: walking speed 0.1, sprint multiplier 1.3, jump impulse 0.42, gravity 0.08, input friction 0.98, ground acceleration factor 0.21600002, horizontal drag 0.91, and vertical drag 0.98. Collision axis resolution is Y then X then Z for full-cube blocks.

This is intentionally a partial vertical slice. It does not yet implement the complete vanilla movement state machine, block-specific friction, crouching, step-up retry, fluids, effects, vehicles, or creative flight. It must be checked against 26.3 before claiming target-version parity.

## Next porting order

1. Audit the 26.3 runtime's registries and chunk representation; replace placeholder IDs only from confirmed target data.
2. Port palette-based chunk section decoding and validate it against real 26.3 data.
3. Replace color-only cubes with the source-informed block model/face pipeline and texture atlas.
4. Extend collision to voxel shapes and step-up behavior, then compare movement traces against the target.
5. Implement the audited WebSocket handshake and packet framing only after the 26.3 protocol differences are known.
6. Add browser persistence, audio, entities, and remaining gameplay as independently tested vertical slices.

## Licensing and attribution

The reference archive's root license identifies its own MIT-licensed project code and separately acknowledges Minecraft-derived code. That does not automatically grant rights to redistribute Mojang code or assets. Keep the Rust implementation independently written, preserve required notices for any permitted dependencies, and verify the applicable upstream terms before copying source or assets. This document records source locations and behavior to audit; it does not reproduce the decompiled source.
