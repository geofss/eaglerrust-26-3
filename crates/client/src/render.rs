//! A small, real voxel renderer built on browser WebGL1.
//!
//! Rust creates the block mesh, owns the camera and submits every frame.
//! This is a renderer milestone, not yet the complete Minecraft client.

use std::{cell::RefCell, collections::{HashMap, HashSet}};
use js_sys::Float32Array;
use wasm_bindgen::{prelude::*, JsCast};
use crate::atlas::{prototype_atlas_pixels, AtlasRect, TextureAtlasLayout, PROTOTYPE_ATLAS_TILE_COUNT, PROTOTYPE_ATLAS_TILE_SIZE};
use crate::chunk::ChunkManager;
use crate::input::InputFrame;
use crate::interaction::{break_block, place_block, target_block, MiningController, MiningUpdate};
use crate::physics::Vec3;
use crate::player::Player;
use crate::world::{BlockPosition, BlockStateId, ChunkPosition, SectionPosition, TerrainGenerator, World};
use web_sys::{
    HtmlCanvasElement, WebGlBuffer, WebGlProgram, WebGlRenderingContext as Gl,
    WebGlShader, WebGlTexture, WebGlUniformLocation,
};

const TERRAIN_VERTEX_FLOATS: usize = 8;

struct SectionGpuMesh {
    buffer: WebGlBuffer,
    vertex_count: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerNetworkState {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

struct Renderer {
    gl: Gl,
    program: WebGlProgram,
    atlas_texture: WebGlTexture,
    atlas_layout: TextureAtlasLayout,
    atlas_sampler_location: WebGlUniformLocation,
    section_meshes: HashMap<SectionPosition, SectionGpuMesh>,
    outline_buffer: WebGlBuffer,
    matrix_location: WebGlUniformLocation,
    position_attribute: u32,
    color_attribute: u32,
    uv_attribute: u32,
    world: World,
    chunk_manager: ChunkManager,
    world_seed: u32,
    player: Player,
    camera_yaw: f32,
    camera_pitch: f32,
    pressed_keys: HashSet<String>,
    last_tick_ms: f64,
    tick_accumulator: f64,
    server_world_active: bool,
    breaking_held: bool,
    mining: MiningController,
    active_dig: Option<(BlockPosition, [i32; 3])>,
    completed_dig: Option<(BlockPosition, [i32; 3])>,
    pending_dig_actions: Vec<(i32, BlockPosition, [i32; 3])>,
}

thread_local! {
    static RENDERER: RefCell<Option<Renderer>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderState {
    Uninitialized,
    Ready,
    Unsupported,
    Lost,
}

impl Default for RenderState {
    fn default() -> Self {
        Self::Uninitialized
    }
}

const VERTEX_SHADER: &str = r#"
attribute vec3 a_position;
attribute vec3 a_color;
attribute vec2 a_uv;
uniform mat4 u_view_projection;
varying vec3 v_color;
varying vec2 v_uv;

void main() {
    gl_Position = u_view_projection * vec4(a_position, 1.0);
    v_color = a_color;
    v_uv = a_uv;
}
"#;

const FRAGMENT_SHADER: &str = r#"
precision mediump float;
varying vec3 v_color;
varying vec2 v_uv;
uniform sampler2D u_atlas;

void main() {
    gl_FragColor = texture2D(u_atlas, v_uv) * vec4(v_color, 1.0);
}
"#;

/// Start WebGL, build the voxel mesh, and draw the first frame.
pub fn start_renderer(canvas_id: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("Window is unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("Document is unavailable"))?;
    let element = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| JsValue::from_str("Canvas element was not found"))?;
    let canvas: HtmlCanvasElement = element
        .dyn_into()
        .map_err(|_| JsValue::from_str("The renderer target is not a canvas"))?;

    let context = canvas
        .get_context("webgl")?
        .ok_or_else(|| JsValue::from_str("WebGL1 is not available in this browser"))?;
    let gl: Gl = context
        .dyn_into()
        .map_err(|_| JsValue::from_str("Could not create a WebGL1 context"))?;

    let program = link_program(&gl)?;
    let (atlas_texture, atlas_layout) = create_prototype_texture_atlas(&gl)?;
    let outline_buffer = gl
        .create_buffer()
        .ok_or_else(|| JsValue::from_str("Could not allocate the block outline buffer"))?;
    let matrix_location = gl
        .get_uniform_location(&program, "u_view_projection")
        .ok_or_else(|| JsValue::from_str("Camera matrix uniform is missing"))?;
    let atlas_sampler_location = gl
        .get_uniform_location(&program, "u_atlas")
        .ok_or_else(|| JsValue::from_str("Texture atlas sampler is missing"))?;

    let world_seed = 0xEA61_2026;
    let generator = TerrainGenerator::new(world_seed, 5, 3);
    let mut world = World::new();
    let mut chunk_manager = ChunkManager::new(1);
    chunk_manager.update(&mut world, BlockPosition::new(0, 0, 0).chunk(), &generator);
    let section_meshes = build_section_gpu_meshes(&gl, &world, atlas_layout)?;
    world.take_dirty_sections();
    let spawn_height = generator.height_at(0, 0).max(1) as f64 + 1.01;
    let player = Player::new(Vec3::new(0.5, spawn_height, 0.5));
    gl.use_program(Some(&program));
    let position = gl.get_attrib_location(&program, "a_position");
    let color = gl.get_attrib_location(&program, "a_color");
    let uv = gl.get_attrib_location(&program, "a_uv");
    if position < 0 || color < 0 || uv < 0 {
        return Err(JsValue::from_str("Terrain shader attributes were not found"));
    }

    let stride = (8 * std::mem::size_of::<f32>()) as i32;
    gl.enable_vertex_attrib_array(position as u32);
    gl.vertex_attrib_pointer_with_i32(position as u32, 3, Gl::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(color as u32);
    gl.vertex_attrib_pointer_with_i32(color as u32, 3, Gl::FLOAT, false, stride, (3 * std::mem::size_of::<f32>()) as i32);
    gl.enable_vertex_attrib_array(uv as u32);
    gl.vertex_attrib_pointer_with_i32(uv as u32, 2, Gl::FLOAT, false, stride, (6 * std::mem::size_of::<f32>()) as i32);
    gl.active_texture(Gl::TEXTURE0);
    gl.bind_texture(Gl::TEXTURE_2D, Some(&atlas_texture));
    gl.uniform1i(Some(&atlas_sampler_location), 0);

    gl.enable(Gl::DEPTH_TEST);
    gl.depth_func(Gl::LEQUAL);
    gl.clear_color(0.48, 0.72, 0.91, 1.0);

    RENDERER.with(|slot| {
        *slot.borrow_mut() = Some(Renderer {
            gl,
            program,
            atlas_texture,
            atlas_layout,
            atlas_sampler_location,
            section_meshes,
            outline_buffer,
            matrix_location,
            position_attribute: position as u32,
            color_attribute: color as u32,
            uv_attribute: uv as u32,
            world,
            chunk_manager,
            world_seed,
            player,
            camera_yaw: 0.0,
            camera_pitch: -0.34,
            pressed_keys: HashSet::new(),
            last_tick_ms: js_sys::Date::now(),
            tick_accumulator: 0.0,
            server_world_active: false,
            breaking_held: false,
            mining: MiningController::default(),
            active_dig: None,
            completed_dig: None,
            pending_dig_actions: Vec::new(),
        });
    });

    resize_renderer(canvas_id)?;
    draw_frame()
}

/// Keep the drawing buffer sharp when the browser viewport changes size.
pub fn resize_renderer(canvas_id: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| JsValue::from_str("Document is unavailable"))?;
    let element = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| JsValue::from_str("Canvas element was not found"))?;
    let canvas: HtmlCanvasElement = element
        .dyn_into()
        .map_err(|_| JsValue::from_str("The renderer target is not a canvas"))?;

    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;
    canvas.set_width(width);
    canvas.set_height(height);

    RENDERER.with(|slot| {
        let borrowed = slot.borrow();
        let renderer = borrowed
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Renderer has not been started"))?;
        renderer.gl.viewport(0, 0, width as i32, height as i32);
        Ok(())
    })
}

/// Update a held keyboard key. Key names are normalized by the browser host.
pub fn set_key(key: &str, pressed: bool) {
    RENDERER.with(|slot| {
        if let Some(renderer) = slot.borrow_mut().as_mut() {
            let key = key.to_ascii_lowercase();
            if pressed {
                renderer.pressed_keys.insert(key);
            } else {
                renderer.pressed_keys.remove(&key);
            }
        }
    });
}

/// Return to a clean local prototype world after the reference connection closes.
pub fn disconnect_reference_world() {
    RENDERER.with(|slot| {
        if let Some(renderer) = slot.borrow_mut().as_mut() {
            for (_, mesh) in renderer.section_meshes.drain() {
                renderer.gl.delete_buffer(Some(&mesh.buffer));
            }
            let radius = renderer.chunk_manager.radius();
            renderer.world = World::new();
            renderer.chunk_manager = ChunkManager::new(radius);
            renderer.server_world_active = false;
            renderer.breaking_held = false;
            renderer.mining.reset();
            renderer.active_dig = None;
            renderer.completed_dig = None;
            renderer.pending_dig_actions.clear();
        }
    });
}

/// Create a fresh local world with a deterministic seed.
pub fn create_singleplayer_world(seed: u32) -> Result<(), JsValue> {
    RENDERER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let renderer = borrowed.as_mut().ok_or_else(|| JsValue::from_str("Renderer has not been started"))?;
        if renderer.server_world_active {
            return Err(JsValue::from_str("Disconnect from the server before creating a local world"));
        }
        let generator = TerrainGenerator::new(seed, 5, 3);
        let mut world = World::new();
        let mut chunk_manager = ChunkManager::new(renderer.chunk_manager.radius());
        chunk_manager.update(&mut world, ChunkPosition::new(0, 0), &generator);
        let meshes = build_section_gpu_meshes(&renderer.gl, &world, renderer.atlas_layout)?;
        for (_, mesh) in renderer.section_meshes.drain() {
            renderer.gl.delete_buffer(Some(&mesh.buffer));
        }
        world.take_dirty_sections();
        let spawn_height = generator.height_at(0, 0).max(1) as f64 + 1.01;
        renderer.world = world;
        renderer.chunk_manager = chunk_manager;
        renderer.world_seed = seed;
        renderer.section_meshes = meshes;
        renderer.player = Player::new(Vec3::new(0.5, spawn_height, 0.5));
        renderer.camera_yaw = 0.0;
        renderer.camera_pitch = -0.34;
        renderer.pressed_keys.clear();
        renderer.tick_accumulator = 0.0;
        renderer.server_world_active = false;
        renderer.breaking_held = false;
        renderer.mining.reset();
        renderer.active_dig = None;
        renderer.completed_dig = None;
        renderer.pending_dig_actions.clear();
        Ok(())
    })
}

/// Serialize the current local world and player spawn state for browser storage.
pub fn save_singleplayer_world() -> Result<Vec<u8>, JsValue> {
    RENDERER.with(|slot| {
        let borrowed = slot.borrow();
        let renderer = borrowed.as_ref().ok_or_else(|| JsValue::from_str("Renderer has not been started"))?;
        if renderer.server_world_active {
            return Err(JsValue::from_str("Multiplayer worlds cannot be saved as singleplayer worlds"));
        }
        let save = crate::singleplayer::SingleplayerSave {
            seed: renderer.world_seed,
            player_position: [
                renderer.player.position.x,
                renderer.player.position.y,
                renderer.player.position.z,
            ],
            yaw: renderer.camera_yaw,
            pitch: renderer.camera_pitch,
            world: renderer.world.clone(),
        };
        save.encode().map_err(|error| JsValue::from_str(&error.to_string()))
    })
}

/// Restore a validated local save and rebuild its GPU meshes before replacing the active world.
pub fn load_singleplayer_world(bytes: &[u8]) -> Result<(), JsValue> {
    let save = crate::singleplayer::SingleplayerSave::decode(bytes)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    RENDERER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let renderer = borrowed.as_mut().ok_or_else(|| JsValue::from_str("Renderer has not been started"))?;
        if renderer.server_world_active {
            return Err(JsValue::from_str("Disconnect from the server before loading a local save"));
        }
        let meshes = build_section_gpu_meshes(&renderer.gl, &save.world, renderer.atlas_layout)?;
        for (_, mesh) in renderer.section_meshes.drain() {
            renderer.gl.delete_buffer(Some(&mesh.buffer));
        }
        let mut world = save.world;
        world.take_dirty_sections();
        let mut chunk_manager = ChunkManager::new(renderer.chunk_manager.radius());
        chunk_manager.adopt_loaded_world(&world);
        renderer.world = world;
        renderer.chunk_manager = chunk_manager;
        renderer.world_seed = save.seed;
        renderer.section_meshes = meshes;
        renderer.player = Player::new(Vec3::new(
            save.player_position[0], save.player_position[1], save.player_position[2],
        ));
        renderer.camera_yaw = save.yaw;
        renderer.camera_pitch = save.pitch.clamp(-1.45, 1.45);
        renderer.player.yaw = f64::from(renderer.camera_yaw);
        renderer.pressed_keys.clear();
        renderer.tick_accumulator = 0.0;
        renderer.server_world_active = false;
        renderer.breaking_held = false;
        renderer.mining.reset();
        renderer.active_dig = None;
        renderer.completed_dig = None;
        renderer.pending_dig_actions.clear();
        Ok(())
    })
}

/// Set or clear the held primary break action. The frame loop handles abort packets.
pub fn set_breaking(held: bool) {
    RENDERER.with(|slot| {
        if let Some(renderer) = slot.borrow_mut().as_mut() {
            renderer.breaking_held = held;
        }
    });
}

/// Current normalized mining progress (0.0 to 1.0).
pub fn mining_progress() -> f32 {
    RENDERER.with(|slot| slot.borrow().as_ref().map(|renderer| renderer.mining.progress()).unwrap_or(0.0))
}

/// Drain queued server dig actions generated by the frame loop.
pub fn take_pending_dig_actions() -> Vec<(i32, BlockPosition, [i32; 3])> {
    RENDERER.with(|slot| slot.borrow_mut().as_mut().map(|renderer| std::mem::take(&mut renderer.pending_dig_actions)).unwrap_or_default())
}

/// Apply pointer or touch-look movement to the player camera.
pub fn look(delta_x: f32, delta_y: f32) {
    RENDERER.with(|slot| {
        if let Some(renderer) = slot.borrow_mut().as_mut() {
            renderer.camera_yaw -= delta_x.clamp(-500.0, 500.0) * 0.003;
            renderer.camera_pitch = (renderer.camera_pitch
                - delta_y.clamp(-500.0, 500.0) * 0.003)
                .clamp(-1.45, 1.45);
            renderer.player.yaw = f64::from(renderer.camera_yaw);
        }
    });
}

/// Render one frame using the Rust player controller and the live prototype world.
pub fn draw_frame() -> Result<(), JsValue> {
    RENDERER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let renderer = borrowed
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Renderer has not been started"))?;

        let forward = (renderer.pressed_keys.contains("w") as i32
            - renderer.pressed_keys.contains("s") as i32) as f32;
        let strafe = (renderer.pressed_keys.contains("d") as i32
            - renderer.pressed_keys.contains("a") as i32) as f32;
        let input = InputFrame {
            forward,
            strafe,
            jump_pressed: renderer.pressed_keys.contains(" "),
            sprint_pressed: renderer.pressed_keys.contains("control")
                || renderer.pressed_keys.contains("shift"),
            ..InputFrame::default()
        };
        renderer.player.yaw = f64::from(renderer.camera_yaw);
        let now_ms = js_sys::Date::now();
        let elapsed = ((now_ms - renderer.last_tick_ms) / 1000.0).clamp(0.0, 0.25);
        renderer.last_tick_ms = now_ms;
        renderer.tick_accumulator += elapsed;
        while renderer.tick_accumulator >= 0.05 {
            renderer.player.tick(&renderer.world, &input);
            let center_chunk = BlockPosition::new(
                renderer.player.position.x.floor() as i32, 0,
                renderer.player.position.z.floor() as i32,
            ).chunk();
            if !renderer.server_world_active {
                let chunk_update = renderer.chunk_manager.update_with_budget(
                    &mut renderer.world,
                    center_chunk,
                    &TerrainGenerator::new(renderer.world_seed, 5, 3),
                    1,
                    1,
                );
                if chunk_update.loaded_columns > 0 || chunk_update.unloaded_columns > 0 {
                    update_dirty_section_meshes(renderer);
                }
            }
            renderer.tick_accumulator -= 0.05;
        }

        // Mining is hold-driven in both modes. The local world applies completed
        // breaks immediately; the reference server receives start/abort/finish and
        // remains authoritative over multiplayer block changes.
        let hit = current_target(renderer);
        if renderer.server_world_active {
            let raw_desired = if renderer.breaking_held {
                hit.map(|target| (target.position, target.face))
            } else { None };
            if renderer.completed_dig.is_some() && raw_desired != renderer.completed_dig {
                renderer.completed_dig = None;
            }
            let desired = if raw_desired == renderer.completed_dig { None } else { raw_desired };
            if desired != renderer.active_dig {
                if let Some(previous) = renderer.active_dig.take() {
                    renderer.pending_dig_actions.push((1, previous.0, previous.1));
                }
                renderer.mining.reset();
                if let Some(next) = desired {
                    match renderer.mining.tick(hit, true, elapsed as f32) {
                        MiningUpdate::Completed { .. } => {
                            renderer.pending_dig_actions.push((0, next.0, next.1));
                            renderer.pending_dig_actions.push((2, next.0, next.1));
                            renderer.completed_dig = Some(next);
                        }
                        _ => {
                            renderer.pending_dig_actions.push((0, next.0, next.1));
                            renderer.active_dig = Some(next);
                        }
                    }
                }
            } else if let Some(active) = renderer.active_dig {
                if matches!(renderer.mining.tick(hit, true, elapsed as f32), MiningUpdate::Completed { .. }) {
                    renderer.pending_dig_actions.push((2, active.0, active.1));
                    renderer.active_dig = None;
                    renderer.completed_dig = Some(active);
                }
            }
        } else {
            if let MiningUpdate::Completed { position, .. } =
                renderer.mining.tick(hit, renderer.breaking_held, elapsed as f32)
            {
                if renderer.world.set_block(position, crate::world::AIR) {
                    update_dirty_section_meshes(renderer);
                }
            }
        }

        let canvas: HtmlCanvasElement = renderer.gl.canvas()
            .ok_or_else(|| JsValue::from_str("WebGL canvas is unavailable"))?
            .dyn_into()
            .map_err(|_| JsValue::from_str("WebGL canvas is not an HTML canvas"))?;
        let width = canvas.width().max(1) as f32;
        let height = canvas.height().max(1) as f32;

        let horizontal = renderer.camera_pitch.cos();
        let direction = [
            renderer.camera_yaw.sin() * horizontal,
            renderer.camera_pitch.sin(),
            -renderer.camera_yaw.cos() * horizontal,
        ];
        let eye = [
            renderer.player.position.x as f32,
            (renderer.player.position.y + 1.62) as f32,
            renderer.player.position.z as f32,
        ];
        let target = [
            eye[0] + direction[0],
            eye[1] + direction[1],
            eye[2] + direction[2],
        ];
        let view = look_at(eye, target, [0.0, 1.0, 0.0]);
        let projection = perspective(60.0_f32.to_radians(), width / height, 0.1, 180.0);
        let view_projection = multiply(projection, view);

        renderer.gl.use_program(Some(&renderer.program));
        let stride = (8 * std::mem::size_of::<f32>()) as i32;
        renderer.gl.uniform_matrix4fv_with_f32_array(
            Some(&renderer.matrix_location),
            false,
            &view_projection,
        );
        renderer.gl.clear(Gl::COLOR_BUFFER_BIT | Gl::DEPTH_BUFFER_BIT);
        renderer.gl.active_texture(Gl::TEXTURE0);
        renderer.gl.bind_texture(Gl::TEXTURE_2D, Some(&renderer.atlas_texture));
        renderer.gl.uniform1i(Some(&renderer.atlas_sampler_location), 0);
        for mesh in renderer.section_meshes.values() {
            renderer.gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&mesh.buffer));
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.position_attribute, 3, Gl::FLOAT, false, stride, 0);
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.color_attribute, 3, Gl::FLOAT, false, stride, (3 * std::mem::size_of::<f32>()) as i32);
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.uv_attribute, 2, Gl::FLOAT, false, stride, (6 * std::mem::size_of::<f32>()) as i32);
            renderer.gl.draw_arrays(Gl::TRIANGLES, 0, mesh.vertex_count);
        }

        if let Some(hit) = current_target(renderer) {
            let outline = block_outline_mesh(hit.position, renderer.atlas_layout);
            let outline_data = Float32Array::from(outline.as_slice());
            renderer.gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&renderer.outline_buffer));
            renderer.gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &outline_data, Gl::DYNAMIC_DRAW);
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.position_attribute, 3, Gl::FLOAT, false, stride, 0);
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.color_attribute, 3, Gl::FLOAT, false, stride, (3 * std::mem::size_of::<f32>()) as i32);
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.uv_attribute, 2, Gl::FLOAT, false, stride, (6 * std::mem::size_of::<f32>()) as i32);
            renderer.gl.draw_arrays(Gl::LINES, 0, (outline.len() / 8) as i32);
        }
        Ok(())
    })
}


/// Snapshot the local simulation state for the reference multiplayer movement pump.
pub fn player_network_state() -> Option<PlayerNetworkState> {
    RENDERER.with(|slot| {
        let borrowed = slot.borrow();
        let renderer = borrowed.as_ref()?;
        Some(PlayerNetworkState {
            x: renderer.player.position.x,
            y: renderer.player.position.y,
            z: renderer.player.position.z,
            yaw: renderer.camera_yaw,
            pitch: renderer.camera_pitch,
            on_ground: renderer.player.on_ground,
        })
    })
}

fn encode_reference_varint(value: i32, out: &mut Vec<u8>) {
    let mut value = value as u32;
    while value & !0x7f != 0 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn pack_reference_block_position(x: i32, y: i32, z: i32) -> i64 {
    ((i64::from(x) & 0x3ff_ffff) << 38)
        | ((i64::from(z) & 0x3ff_ffff) << 12)
        | (i64::from(y) & 0xfff)
}

fn reference_face_id(face: [i32; 3]) -> u8 {
    match face {
        [0, -1, 0] => 0,
        [0, 1, 0] => 1,
        [0, 0, -1] => 2,
        [0, 0, 1] => 3,
        [-1, 0, 0] => 4,
        [1, 0, 0] => 5,
        _ => 1,
    }
}

/// Pure encoder for the reference Player Action payload used by dig lifecycle tests.
fn encode_reference_dig_action(
    status: i32,
    position: BlockPosition,
    face: [i32; 3],
) -> Option<Vec<u8>> {
    if status != 0 && status != 1 && status != 2 { return None; }
    let mut payload = Vec::with_capacity(10);
    encode_reference_varint(status, &mut payload);
    payload.extend_from_slice(&pack_reference_block_position(
        position.x, position.y, position.z,
    ).to_be_bytes());
    payload.push(reference_face_id(face));
    Some(payload)
}

/// Build a reference Player Action payload for a specific captured dig target.
/// Status values 0/1/2 are reference-protocol assumptions, not 26.3-verified.
pub fn server_dig_action_payload_at(
    status: i32,
    position: BlockPosition,
    face: [i32; 3],
) -> Option<Vec<u8>> {
    RENDERER.with(|slot| {
        let borrowed = slot.borrow();
        let renderer = borrowed.as_ref()?;
        if !renderer.server_world_active { return None; }
        encode_reference_dig_action(status, position, face)
    })
}

/// Build a reference Player Action payload for the currently targeted block.
pub fn server_dig_action_payload(status: i32) -> Option<Vec<u8>> {
    let target = RENDERER.with(|slot| {
        let borrowed = slot.borrow();
        let renderer = borrowed.as_ref()?;
        if !renderer.server_world_active { return None; }
        let hit = current_target(renderer)?;
        Some((hit.position, hit.face))
    })?;
    server_dig_action_payload_at(status, target.0, target.1)
}

/// Build the audited reference Use Item On payload for the targeted block face.
pub fn server_use_item_on_payload() -> Option<Vec<u8>> {
    RENDERER.with(|slot| {
        let borrowed = slot.borrow();
        let renderer = borrowed.as_ref()?;
        if !renderer.server_world_active { return None; }
        let hit = current_target(renderer)?;
        let mut payload = Vec::with_capacity(9);
        payload.extend_from_slice(&pack_reference_block_position(
            hit.position.x, hit.position.y, hit.position.z,
        ).to_be_bytes());
        payload.push(reference_face_id(hit.face));
        Some(payload)
    })
}

/// Apply one decoded 26.1.2 reference play packet to the live renderer.
///
/// The first server Play Login clears the procedural demo terrain and switches
/// chunk streaming to server-authoritative mode. Chunk packets are validated
/// in full before their sections are installed and remeshed.
pub fn apply_reference_play_packet(packet_id: i32, payload: &[u8]) -> Result<(), JsValue> {
    RENDERER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let renderer = borrowed.as_mut()
            .ok_or_else(|| JsValue::from_str("Renderer has not been started"))?;
        let packet = crate::reference_play::decode_reference_play_packet(packet_id, payload)
            .map_err(|error| JsValue::from_str(&format!("reference play packet rejected: {error:?}")))?;
        match packet {
            crate::reference_play::ReferencePlayPacket::Login(login) => {
                for (_, mesh) in renderer.section_meshes.drain() {
                    renderer.gl.delete_buffer(Some(&mesh.buffer));
                }
                renderer.world = World::new();
                renderer.server_world_active = true;
                renderer.breaking_held = false;
                renderer.mining.reset();
                renderer.active_dig = None;
                renderer.completed_dig = None;
                renderer.pending_dig_actions.clear();
                renderer.player.position = Vec3::new(login.spawn_x, login.spawn_y, login.spawn_z);
            }
            crate::reference_play::ReferencePlayPacket::Chunk(chunk) => {
                for section in chunk.sections {
                    renderer.world.replace_section(
                        SectionPosition::new(
                            chunk.chunk_x,
                            crate::reference_play::reference_section_y_to_world(section.section_y),
                            chunk.chunk_z,
                        ),
                        section.data,
                    );
                }
                update_dirty_section_meshes(renderer);
            }
            crate::reference_play::ReferencePlayPacket::BlockUpdate { position, state } => {
                renderer.world.set_block(position, state);
                update_dirty_section_meshes(renderer);
            }
            crate::reference_play::ReferencePlayPacket::UnloadChunk { chunk_x, chunk_z } => {
                renderer.world.remove_column(chunk_x, chunk_z);
                update_dirty_section_meshes(renderer);
            }
            crate::reference_play::ReferencePlayPacket::PlayerPosition {
                x, y, z, yaw, pitch, relative_flags, ..
            } => {
                let position = renderer.player.position;
                renderer.player.position = Vec3::new(
                    if relative_flags & 0x01 != 0 { position.x + x } else { x },
                    if relative_flags & 0x02 != 0 { position.y + y } else { y },
                    if relative_flags & 0x04 != 0 { position.z + z } else { z },
                );
                // The local camera uses yaw 0 toward -Z and positive pitch upward;
                // the reference server uses vanilla degrees (yaw 180 toward -Z,
                // positive pitch downward).
                let local_yaw_delta = -yaw.to_radians();
                let local_pitch_delta = -pitch.to_radians();
                renderer.camera_yaw = if relative_flags & 0x08 != 0 {
                    renderer.camera_yaw + local_yaw_delta
                } else {
                    (yaw - 180.0).to_radians()
                };
                renderer.camera_pitch = if relative_flags & 0x10 != 0 {
                    renderer.camera_pitch + local_pitch_delta
                } else {
                    local_pitch_delta
                }.clamp(-1.5533, 1.5533);
                renderer.player.yaw = f64::from(renderer.camera_yaw);
                renderer.player.velocity = Vec3::default();
                renderer.player.on_ground = false;
            }
            crate::reference_play::ReferencePlayPacket::SetContainerSlot { .. } => {}
            crate::reference_play::ReferencePlayPacket::Unhandled { .. } => {}
        }
        Ok(())
    })
}

/// Return the currently targeted prototype block ID, or zero when no block is targeted.
pub fn target_state() -> u32 {
    RENDERER.with(|slot| {
        let borrowed = slot.borrow();
        let Some(renderer) = borrowed.as_ref() else { return 0; };
        current_target(renderer).map(|hit| hit.state.0).unwrap_or(0)
    })
}

/// Break the block currently under the crosshair and upload the changed world mesh.
pub fn break_target() -> bool {
    RENDERER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(renderer) = borrowed.as_mut() else { return false; };
        let Some(hit) = current_target(renderer) else { return false; };
        if !break_block(&mut renderer.world, hit) { return false; }
        update_dirty_section_meshes(renderer)
    })
}

/// Place the selected prototype block against the face under the crosshair.
pub fn place_target(state: u32) -> bool {
    RENDERER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(renderer) = borrowed.as_mut() else { return false; };
        let Some(hit) = current_target(renderer) else { return false; };
        if !place_block(&mut renderer.world, hit, BlockStateId(state)) { return false; }
        update_dirty_section_meshes(renderer)
    })
}

fn current_target(renderer: &Renderer) -> Option<crate::raycast::BlockHit> {
    let horizontal = renderer.camera_pitch.cos();
    let direction = [
        f64::from(renderer.camera_yaw.sin() * horizontal),
        f64::from(renderer.camera_pitch.sin()),
        f64::from(-renderer.camera_yaw.cos() * horizontal),
    ];
    let eye = [
        renderer.player.position.x,
        renderer.player.position.y + 1.62,
        renderer.player.position.z,
    ];
    target_block(&renderer.world, eye, direction, 5.0)
}

fn block_outline_mesh(position: BlockPosition, atlas: TextureAtlasLayout) -> Vec<f32> {
    let x = position.x as f32 - 0.003;
    let y = position.y as f32 - 0.003;
    let z = position.z as f32 - 0.003;
    let x1 = x + 1.006;
    let y1 = y + 1.006;
    let z1 = z + 1.006;
    let corners = [
        [x, y, z], [x1, y, z], [x, y1, z], [x1, y1, z],
        [x, y, z1], [x1, y, z1], [x, y1, z1], [x1, y1, z1],
    ];
    let edges = [
        (0, 1), (1, 3), (3, 2), (2, 0),
        (4, 5), (5, 7), (7, 6), (6, 4),
        (0, 4), (1, 5), (2, 6), (3, 7),
    ];
    let white = atlas.rect(8).expect("outline tile exists in prototype atlas");
    let uv = [(white.u_min + white.u_max) * 0.5, (white.v_min + white.v_max) * 0.5];
    let mut vertices = Vec::with_capacity(24 * 8);
    for (a, b) in edges {
        for point in [corners[a], corners[b]] {
            vertices.extend_from_slice(&point);
            vertices.extend_from_slice(&[1.0, 1.0, 1.0]);
            vertices.extend_from_slice(&uv);
        }
    }
    vertices
}

fn mesh_vertex_count(vertices: &[f32]) -> i32 {
    debug_assert_eq!(vertices.len() % TERRAIN_VERTEX_FLOATS, 0);
    (vertices.len() / TERRAIN_VERTEX_FLOATS) as i32
}

fn build_section_gpu_meshes(gl: &Gl, world: &World, atlas_layout: TextureAtlasLayout) -> Result<HashMap<SectionPosition, SectionGpuMesh>, JsValue> {
    let mut meshes = HashMap::new();
    for section in world.loaded_sections() {
        let vertices = build_world_section_mesh_with_atlas(world, section, atlas_layout);
        if vertices.is_empty() { continue; }
        let buffer = gl.create_buffer().ok_or_else(|| JsValue::from_str("Could not allocate a chunk section buffer"))?;
        let data = Float32Array::from(vertices.as_slice());
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&buffer));
        gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &data, Gl::STATIC_DRAW);
        meshes.insert(section, SectionGpuMesh { buffer, vertex_count: mesh_vertex_count(&vertices) });
    }
    Ok(meshes)
}

/// Rebuild only sections invalidated by a world edit; neighbouring section
/// meshes are included by World::set_block when an edit touches a boundary.
fn update_dirty_section_meshes(renderer: &mut Renderer) -> bool {
    let dirty = renderer.world.take_dirty_sections();
    for section in dirty {
        let vertices = if renderer.world.is_section_loaded(section) {
            build_world_section_mesh_with_atlas(&renderer.world, section, renderer.atlas_layout)
        } else {
            Vec::new()
        };
        if vertices.is_empty() {
            if let Some(old) = renderer.section_meshes.remove(&section) {
                renderer.gl.delete_buffer(Some(&old.buffer));
            }
            continue;
        }
        let buffer = if let Some(existing) = renderer.section_meshes.get(&section) {
            existing.buffer.clone()
        } else {
            let Some(buffer) = renderer.gl.create_buffer() else { return false; };
            buffer
        };
        let data = Float32Array::from(vertices.as_slice());
        renderer.gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&buffer));
        renderer.gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &data, Gl::DYNAMIC_DRAW);
        renderer.section_meshes.insert(section, SectionGpuMesh {
            buffer,
            vertex_count: mesh_vertex_count(&vertices),
        });
    }
    let unloaded: Vec<_> = renderer.section_meshes.keys().copied()
        .filter(|section| !renderer.world.is_section_loaded(*section)).collect();
    for section in unloaded {
        if let Some(mesh) = renderer.section_meshes.remove(&section) {
            renderer.gl.delete_buffer(Some(&mesh.buffer));
        }
    }
    true
}

fn compile_shader(gl: &Gl, kind: u32, source: &str) -> Result<WebGlShader, JsValue> {
    let shader = gl
        .create_shader(kind)
        .ok_or_else(|| JsValue::from_str("Could not create WebGL shader"))?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    if gl
        .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        let message = gl
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| "Unknown shader compilation error".to_owned());
        Err(JsValue::from_str(&message))
    }
}

fn link_program(gl: &Gl) -> Result<WebGlProgram, JsValue> {
    let vertex = compile_shader(gl, Gl::VERTEX_SHADER, VERTEX_SHADER)?;
    let fragment = compile_shader(gl, Gl::FRAGMENT_SHADER, FRAGMENT_SHADER)?;
    let program = gl
        .create_program()
        .ok_or_else(|| JsValue::from_str("Could not create WebGL program"))?;

    gl.attach_shader(&program, &vertex);
    gl.attach_shader(&program, &fragment);
    gl.link_program(&program);

    if gl
        .get_program_parameter(&program, Gl::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        let message = gl
            .get_program_info_log(&program)
            .unwrap_or_else(|| "Unknown shader link error".to_owned());
        Err(JsValue::from_str(&message))
    }
}

// Each vertex stores position (xyz) followed by a face colour (rgb).
/// Build a sample section using the same dense block storage used by the world model.
#[cfg(test)]
fn build_terrain_world() -> World {
    let mut world = World::new();
    let generator = TerrainGenerator::new(0xEA61_2026, 5, 3);
    generator.generate_region(&mut world, -24, -24, 48, 48);
    world
}

#[cfg(test)]
fn build_terrain_mesh() -> Vec<f32> {
    build_world_mesh(&build_terrain_world())
}

#[cfg(test)]
fn build_world_mesh(world: &World) -> Vec<f32> {
    let mut vertices = Vec::new();
    for section_pos in world.loaded_sections() {
        vertices.extend(build_world_section_mesh(world, section_pos));
    }
    vertices
}

/// Mesh one section while checking neighbouring blocks in the complete world.
/// This suppresses shared faces even when the neighbour is in another section.
fn prototype_atlas_layout() -> TextureAtlasLayout {
    TextureAtlasLayout::new(PROTOTYPE_ATLAS_TILE_COUNT, PROTOTYPE_ATLAS_TILE_SIZE, 4096)
        .expect("fixed prototype atlas fits WebGL texture limits")
}

fn create_prototype_texture_atlas(gl: &Gl) -> Result<(WebGlTexture, TextureAtlasLayout), JsValue> {
    let max_size = gl.get_parameter(Gl::MAX_TEXTURE_SIZE)?.as_f64().unwrap_or(2048.0).max(1.0) as u32;
    let layout = TextureAtlasLayout::new(PROTOTYPE_ATLAS_TILE_COUNT, PROTOTYPE_ATLAS_TILE_SIZE, max_size)
        .map_err(|_| JsValue::from_str("The browser's maximum texture size is too small"))?;
    let pixels = prototype_atlas_pixels(layout);
    let (width, height) = layout.dimensions();
    let texture = gl.create_texture().ok_or_else(|| JsValue::from_str("Could not allocate the block texture atlas"))?;
    gl.bind_texture(Gl::TEXTURE_2D, Some(&texture));
    gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
        Gl::TEXTURE_2D, 0, Gl::RGBA as i32, width as i32, height as i32, 0,
        Gl::RGBA, Gl::UNSIGNED_BYTE, Some(&pixels),
    )?;
    gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::NEAREST as i32);
    gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::NEAREST as i32);
    gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
    gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
    Ok((texture, layout))
}

fn build_world_section_mesh(world: &World, section_pos: SectionPosition) -> Vec<f32> {
    build_world_section_mesh_with_atlas(world, section_pos, prototype_atlas_layout())
}

/// Mesh one section while checking neighbouring blocks in the complete world.
fn build_world_section_mesh_with_atlas(world: &World, section_pos: SectionPosition, atlas: TextureAtlasLayout) -> Vec<f32> {
    let mut vertices = Vec::new();
    for y in 0..16_u8 {
        for z in 0..16_u8 {
            for x in 0..16_u8 {
                let world_pos = BlockPosition::new(
                    section_pos.x * 16 + i32::from(x),
                    section_pos.y * 16 + i32::from(y),
                    section_pos.z * 16 + i32::from(z),
                );
                let state = world.get_block(world_pos);
                if state == BlockStateId(0) { continue; }
                let (top_tile, side_tile, bottom_tile) = block_face_tiles(state);
                let top_uv = atlas.rect(top_tile).expect("top tile exists");
                let side_uv = atlas.rect(side_tile).expect("side tile exists");
                let bottom_uv = atlas.rect(bottom_tile).expect("bottom tile exists");
                let x0 = world_pos.x as f32; let x1 = x0 + 1.0;
                let y0 = world_pos.y as f32; let y1 = y0 + 1.0;
                let z0 = world_pos.z as f32; let z1 = z0 + 1.0;
                let top = [1.0, 1.0, 1.0];
                let side_a = shade(top, 0.78);
                let side_b = shade(top, 0.88);
                let side_c = shade(top, 0.68);
                let bottom = shade(top, 0.52);
                if !world_is_solid(world, world_pos.x, world_pos.y + 1, world_pos.z) {
                    quad(&mut vertices, [[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]], top, top_uv);
                }
                if !world_is_solid(world, world_pos.x, world_pos.y - 1, world_pos.z) {
                    quad(&mut vertices, [[x0,y0,z1],[x1,y0,z1],[x1,y0,z0],[x0,y0,z0]], bottom, bottom_uv);
                }
                if !world_is_solid(world, world_pos.x - 1, world_pos.y, world_pos.z) {
                    quad(&mut vertices, [[x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]], side_a, side_uv);
                }
                if !world_is_solid(world, world_pos.x + 1, world_pos.y, world_pos.z) {
                    quad(&mut vertices, [[x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]], side_b, side_uv);
                }
                if !world_is_solid(world, world_pos.x, world_pos.y, world_pos.z - 1) {
                    quad(&mut vertices, [[x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]], side_c, side_uv);
                }
                if !world_is_solid(world, world_pos.x, world_pos.y, world_pos.z + 1) {
                    quad(&mut vertices, [[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]], side_a, side_uv);
                }
            }
        }
    }
    vertices
}

/// Development atlas tile mapping; these values are not 26.3 registry IDs.
fn block_face_tiles(state: BlockStateId) -> (u32, u32, u32) {
    match state.0 {
        1 => (0, 1, 2), 2 => (2, 2, 2), 3 => (3, 3, 3),
        4 => (4, 4, 4), 5 => (5, 5, 5), 6 => (6, 6, 6),
        7 => (7, 7, 7), _ => (9, 9, 9),
    }
}

fn quad(vertices: &mut Vec<f32>, corners: [[f32; 3]; 4], color: [f32; 3], rect: AtlasRect) {
    let uvs = [
        [rect.u_min, rect.v_min], [rect.u_max, rect.v_min],
        [rect.u_max, rect.v_max], [rect.u_min, rect.v_max],
    ];
    for index in [0, 1, 2, 0, 2, 3] {
        vertices.extend_from_slice(&corners[index]);
        vertices.extend_from_slice(&color);
        vertices.extend_from_slice(&uvs[index]);
    }
}

fn shade(color: [f32; 3], factor: f32) -> [f32; 3] {
    [color[0] * factor, color[1] * factor, color[2] * factor]
}

fn world_is_solid(world: &World, x: i32, y: i32, z: i32) -> bool {
    world.get_block(BlockPosition::new(x, y, z)) != BlockStateId(0)
}

fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov * 0.5).tan();
    let range = 1.0 / (near - far);
    [
        f / aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, (far + near) * range, -1.0,
        0.0, 0.0, (2.0 * far * near) * range, 0.0,
    ]
}

fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let forward = normalize(subtract(target, eye));
    let side = normalize(cross(forward, up));
    let corrected_up = cross(side, forward);

    [
        side[0], corrected_up[0], -forward[0], 0.0,
        side[1], corrected_up[1], -forward[1], 0.0,
        side[2], corrected_up[2], -forward[2], 0.0,
        -dot(side, eye), -dot(corrected_up, eye), dot(forward, eye), 1.0,
    ]
}

fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut result = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            result[column * 4 + row] = (0..4)
                .map(|index| a[index * 4 + row] * b[column * 4 + index])
                .sum();
        }
    }
    result
}

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(vector: [f32; 3]) -> [f32; 3] {
    let length = dot(vector, vector).sqrt().max(0.0001);
    [vector[0] / length, vector[1] / length, vector[2] / length]
}


#[cfg(test)]
mod tests {
    use super::{block_face_tiles, build_terrain_mesh, build_world_section_mesh, mesh_vertex_count};
    use crate::world::{BlockPosition, BlockStateId, SectionPosition, World};

    #[test]
    fn gpu_vertex_count_uses_position_color_and_uv_stride() {
        let vertices = vec![0.0; 8 * 6];
        assert_eq!(mesh_vertex_count(&vertices), 6);
    }

    #[test]
    fn voxel_mesh_has_complete_triangle_vertices() {
        let mesh = build_terrain_mesh();
        assert!(!mesh.is_empty());
        assert_eq!(mesh.len() % 24, 0, "each triangle has three xyz+rgb+uv vertices");
        assert!(mesh.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn mesher_hides_faces_across_section_boundaries() {
        let mut world = World::new();
        world.set_block(BlockPosition::new(15, 4, 4), BlockStateId(1));
        world.set_block(BlockPosition::new(16, 4, 4), BlockStateId(1));

        let left = build_world_section_mesh(&world, SectionPosition::new(0, 0, 0));
        let right = build_world_section_mesh(&world, SectionPosition::new(1, 0, 0));

        // Each isolated cube has six faces; the shared face is culled on both sides.
        assert_eq!(left.len(), 5 * 6 * 8);
        assert_eq!(right.len(), 5 * 6 * 8);
    }

    #[test]
    fn block_states_map_to_face_specific_atlas_tiles() {
        assert_eq!(block_face_tiles(BlockStateId(1)), (0, 1, 2));
        assert_eq!(block_face_tiles(BlockStateId(2)), (2, 2, 2));
        assert_eq!(block_face_tiles(BlockStateId(3)), (3, 3, 3));
        assert_eq!(block_face_tiles(BlockStateId(999)), (9, 9, 9));
    }

    #[test]
    fn empty_section_produces_no_geometry() {
        let world = World::new();
        let mesh = build_world_section_mesh(&world, SectionPosition::new(0, 0, 0));
        assert!(mesh.is_empty());
    }

    #[test]
    fn reference_dig_payload_encodes_each_lifecycle_status() {
        let position = BlockPosition::new(-2, 64, 3);
        for status in 0..=2 {
            let payload = super::encode_reference_dig_action(status, position, [0, 1, 0]).unwrap();
            assert_eq!(payload.len(), 10);
            assert_eq!(payload[0], status as u8);
            assert_eq!(payload[9], 1, "up face maps to reference face 1");
        }
    }

    #[test]
    fn reference_dig_payload_rejects_unknown_status() {
        assert!(super::encode_reference_dig_action(
            3, BlockPosition::new(0, 0, 0), [0, 1, 0]
        ).is_none());
    }
}
