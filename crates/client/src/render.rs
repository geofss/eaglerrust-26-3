//! A small, real voxel renderer built on browser WebGL1.
//!
//! Rust creates the block mesh, owns the camera and submits every frame.
//! This is a renderer milestone, not yet the complete Minecraft client.

use std::{cell::RefCell, collections::{HashMap, HashSet}};
use js_sys::Float32Array;
use wasm_bindgen::{prelude::*, JsCast};
use crate::chunk::ChunkManager;
use crate::input::InputFrame;
use crate::interaction::{break_block, place_block, target_block};
use crate::physics::Vec3;
use crate::player::Player;
use crate::world::{BlockPosition, BlockStateId, SectionPosition, TerrainGenerator, World};
use web_sys::{
    HtmlCanvasElement, WebGlBuffer, WebGlProgram, WebGlRenderingContext as Gl,
    WebGlShader, WebGlUniformLocation,
};

struct SectionGpuMesh {
    buffer: WebGlBuffer,
    vertex_count: i32,
}

struct Renderer {
    gl: Gl,
    program: WebGlProgram,
    section_meshes: HashMap<SectionPosition, SectionGpuMesh>,
    outline_buffer: WebGlBuffer,
    matrix_location: WebGlUniformLocation,
    position_attribute: u32,
    color_attribute: u32,
    world: World,
    chunk_manager: ChunkManager,
    player: Player,
    camera_yaw: f32,
    camera_pitch: f32,
    pressed_keys: HashSet<String>,
    last_tick_ms: f64,
    tick_accumulator: f64,
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
uniform mat4 u_view_projection;
varying vec3 v_color;

void main() {
    gl_Position = u_view_projection * vec4(a_position, 1.0);
    v_color = a_color;
}
"#;

const FRAGMENT_SHADER: &str = r#"
precision mediump float;
varying vec3 v_color;

void main() {
    gl_FragColor = vec4(v_color, 1.0);
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
    let outline_buffer = gl
        .create_buffer()
        .ok_or_else(|| JsValue::from_str("Could not allocate the block outline buffer"))?;
    let matrix_location = gl
        .get_uniform_location(&program, "u_view_projection")
        .ok_or_else(|| JsValue::from_str("Camera matrix uniform is missing"))?;

    let generator = TerrainGenerator::new(0xEA61_2026, 5, 3);
    let mut world = World::new();
    let mut chunk_manager = ChunkManager::new(1);
    chunk_manager.update(&mut world, BlockPosition::new(0, 0, 0).chunk(), &generator);
    let section_meshes = build_section_gpu_meshes(&gl, &world)?;
    world.take_dirty_sections();
    let spawn_height = TerrainGenerator::new(0xEA61_2026, 5, 3).height_at(0, 0).max(1) as f64 + 1.01;
    let player = Player::new(Vec3::new(0.5, spawn_height, 0.5));
    gl.use_program(Some(&program));
    let position = gl.get_attrib_location(&program, "a_position");
    let color = gl.get_attrib_location(&program, "a_color");
    if position < 0 || color < 0 {
        return Err(JsValue::from_str("Terrain shader attributes were not found"));
    }

    let stride = (6 * std::mem::size_of::<f32>()) as i32;
    gl.enable_vertex_attrib_array(position as u32);
    gl.vertex_attrib_pointer_with_i32(position as u32, 3, Gl::FLOAT, false, stride, 0);
    gl.enable_vertex_attrib_array(color as u32);
    gl.vertex_attrib_pointer_with_i32(
        color as u32,
        3,
        Gl::FLOAT,
        false,
        stride,
        (3 * std::mem::size_of::<f32>()) as i32,
    );

    gl.enable(Gl::DEPTH_TEST);
    gl.depth_func(Gl::LEQUAL);
    gl.clear_color(0.48, 0.72, 0.91, 1.0);

    RENDERER.with(|slot| {
        *slot.borrow_mut() = Some(Renderer {
            gl,
            program,
            section_meshes,
            outline_buffer,
            matrix_location,
            position_attribute: position as u32,
            color_attribute: color as u32,
            world,
            chunk_manager,
            player,
            camera_yaw: 0.0,
            camera_pitch: -0.34,
            pressed_keys: HashSet::new(),
            last_tick_ms: js_sys::Date::now(),
            tick_accumulator: 0.0,
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
            let chunk_update = renderer.chunk_manager.update_with_budget(
                &mut renderer.world,
                center_chunk,
                &TerrainGenerator::new(0xEA61_2026, 5, 3),
                1,
                1,
            );
            if chunk_update.loaded_columns > 0 || chunk_update.unloaded_columns > 0 {
                update_dirty_section_meshes(renderer);
            }
            renderer.tick_accumulator -= 0.05;
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
        let stride = (6 * std::mem::size_of::<f32>()) as i32;
        renderer.gl.uniform_matrix4fv_with_f32_array(
            Some(&renderer.matrix_location),
            false,
            &view_projection,
        );
        renderer.gl.clear(Gl::COLOR_BUFFER_BIT | Gl::DEPTH_BUFFER_BIT);
        for mesh in renderer.section_meshes.values() {
            renderer.gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&mesh.buffer));
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.position_attribute, 3, Gl::FLOAT, false, stride, 0);
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.color_attribute, 3, Gl::FLOAT, false, stride, (3 * std::mem::size_of::<f32>()) as i32);
            renderer.gl.draw_arrays(Gl::TRIANGLES, 0, mesh.vertex_count);
        }

        if let Some(hit) = current_target(renderer) {
            let outline = block_outline_mesh(hit.position);
            let outline_data = Float32Array::from(outline.as_slice());
            renderer.gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&renderer.outline_buffer));
            renderer.gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &outline_data, Gl::DYNAMIC_DRAW);
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.position_attribute, 3, Gl::FLOAT, false, stride, 0);
            renderer.gl.vertex_attrib_pointer_with_i32(renderer.color_attribute, 3, Gl::FLOAT, false, stride, (3 * std::mem::size_of::<f32>()) as i32);
            renderer.gl.draw_arrays(Gl::LINES, 0, (outline.len() / 6) as i32);
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

fn block_outline_mesh(position: BlockPosition) -> Vec<f32> {
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
    let mut vertices = Vec::with_capacity(24 * 6);
    for (a, b) in edges {
        for point in [corners[a], corners[b]] {
            vertices.extend_from_slice(&point);
            vertices.extend_from_slice(&[1.0, 1.0, 1.0]);
        }
    }
    vertices
}

fn build_section_gpu_meshes(gl: &Gl, world: &World) -> Result<HashMap<SectionPosition, SectionGpuMesh>, JsValue> {
    let mut meshes = HashMap::new();
    for section in world.loaded_sections() {
        let vertices = build_world_section_mesh(world, section);
        if vertices.is_empty() { continue; }
        let buffer = gl.create_buffer().ok_or_else(|| JsValue::from_str("Could not allocate a chunk section buffer"))?;
        let data = Float32Array::from(vertices.as_slice());
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&buffer));
        gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &data, Gl::STATIC_DRAW);
        meshes.insert(section, SectionGpuMesh { buffer, vertex_count: (vertices.len() / 6) as i32 });
    }
    Ok(meshes)
}

/// Rebuild only sections invalidated by a world edit; neighbouring section
/// meshes are included by World::set_block when an edit touches a boundary.
fn update_dirty_section_meshes(renderer: &mut Renderer) -> bool {
    let dirty = renderer.world.take_dirty_sections();
    for section in dirty {
        let vertices = if renderer.world.is_section_loaded(section) {
            build_world_section_mesh(&renderer.world, section)
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
            vertex_count: (vertices.len() / 6) as i32,
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
fn build_world_section_mesh(world: &World, section_pos: SectionPosition) -> Vec<f32> {
    let mut vertices = Vec::new();

    for y in 0..16_u8 {
        for z in 0..16_u8 {
            for x in 0..16_u8 {
                let world_pos = BlockPosition::new(
                    section_pos.x * 16 + i32::from(x),
                    section_pos.y * 16 + i32::from(y),
                    section_pos.z * 16 + i32::from(z),
                );
                if world.get_block(world_pos) == BlockStateId(0) {
                    continue;
                }

                let x0 = world_pos.x as f32;
                let x1 = x0 + 1.0;
                let y0 = world_pos.y as f32;
                let y1 = y0 + 1.0;
                let z0 = world_pos.z as f32;
                let z1 = z0 + 1.0;

                let (top_base, side_base, bottom_base) = block_palette(world.get_block(world_pos));
                let top = shade(top_base, 1.0);
                let side_a = shade(side_base, 0.78);
                let side_b = shade(side_base, 0.88);
                let side_c = shade(side_base, 0.68);
                let bottom = shade(bottom_base, 0.52);

                if !world_is_solid(world, world_pos.x, world_pos.y + 1, world_pos.z) {
                    quad(&mut vertices, [[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]], top);
                }
                if !world_is_solid(world, world_pos.x, world_pos.y - 1, world_pos.z) {
                    quad(&mut vertices, [[x0,y0,z1],[x1,y0,z1],[x1,y0,z0],[x0,y0,z0]], bottom);
                }
                if !world_is_solid(world, world_pos.x - 1, world_pos.y, world_pos.z) {
                    quad(&mut vertices, [[x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]], side_a);
                }
                if !world_is_solid(world, world_pos.x + 1, world_pos.y, world_pos.z) {
                    quad(&mut vertices, [[x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]], side_b);
                }
                if !world_is_solid(world, world_pos.x, world_pos.y, world_pos.z - 1) {
                    quad(&mut vertices, [[x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]], side_c);
                }
                if !world_is_solid(world, world_pos.x, world_pos.y, world_pos.z + 1) {
                    quad(&mut vertices, [[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]], side_a);
                }
            }
        }
    }

    vertices
}


/// Temporary material palette keyed by internal block-state IDs.
fn block_palette(state: BlockStateId) -> ([f32; 3], [f32; 3], [f32; 3]) {
    match state.0 {
        1 => ([0.30, 0.72, 0.24], [0.48, 0.34, 0.20], [0.30, 0.21, 0.13]), // grass
        2 => ([0.53, 0.37, 0.23], [0.53, 0.37, 0.23], [0.34, 0.24, 0.16]), // dirt
        3 => ([0.55, 0.57, 0.60], [0.55, 0.57, 0.60], [0.34, 0.35, 0.37]), // stone
        4 => ([0.12, 0.48, 0.78], [0.10, 0.38, 0.66], [0.06, 0.24, 0.42]), // water
        5 => ([0.86, 0.78, 0.52], [0.76, 0.66, 0.42], [0.55, 0.46, 0.30]), // sand
        6 => ([0.43, 0.27, 0.13], [0.34, 0.20, 0.10], [0.22, 0.13, 0.07]), // log
        7 => ([0.16, 0.52, 0.19], [0.12, 0.42, 0.14], [0.08, 0.30, 0.10]), // leaves
        _ => ([0.80, 0.20, 0.80], [0.65, 0.15, 0.65], [0.35, 0.08, 0.35]),
    }
}

fn shade(color: [f32; 3], factor: f32) -> [f32; 3] {
    [color[0] * factor, color[1] * factor, color[2] * factor]
}

fn world_is_solid(world: &World, x: i32, y: i32, z: i32) -> bool {
    world.get_block(BlockPosition::new(x, y, z)) != BlockStateId(0)
}

fn quad(vertices: &mut Vec<f32>, corners: [[f32; 3]; 4], color: [f32; 3]) {
    for index in [0, 1, 2, 0, 2, 3] {
        vertices.extend_from_slice(&corners[index]);
        vertices.extend_from_slice(&color);
    }
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
    use super::{block_palette, build_terrain_mesh, build_world_section_mesh};
    use crate::world::{BlockPosition, BlockStateId, SectionPosition, World};

    #[test]
    fn voxel_mesh_has_complete_triangle_vertices() {
        let mesh = build_terrain_mesh();
        assert!(!mesh.is_empty());
        assert_eq!(mesh.len() % 18, 0, "each triangle has three xyz+rgb vertices");
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
        assert_eq!(left.len(), 5 * 6 * 6);
        assert_eq!(right.len(), 5 * 6 * 6);
    }

    #[test]
    fn block_states_use_distinct_material_palettes() {
        let grass = block_palette(BlockStateId(1));
        let dirt = block_palette(BlockStateId(2));
        let stone = block_palette(BlockStateId(3));
        assert_ne!(grass.0, dirt.0);
        assert_ne!(dirt.0, stone.0);
        assert!(grass.0.iter().all(|channel| (0.0..=1.0).contains(channel)));
    }

    #[test]
    fn empty_section_produces_no_geometry() {
        let world = World::new();
        let mesh = build_world_section_mesh(&world, SectionPosition::new(0, 0, 0));
        assert!(mesh.is_empty());
    }
}
