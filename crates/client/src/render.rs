//! A small, real voxel renderer built on browser WebGL1.
//!
//! Rust creates the block mesh, owns the camera and submits every frame.
//! This is a renderer milestone, not yet the complete Minecraft client.

use std::cell::RefCell;
use js_sys::Float32Array;
use wasm_bindgen::{prelude::*, JsCast};
use crate::world::{BlockPosition, BlockStateId, ChunkSection, LocalBlockPosition, SectionPosition, World};
use web_sys::{
    HtmlCanvasElement, WebGlBuffer, WebGlProgram, WebGlRenderingContext as Gl,
    WebGlShader, WebGlUniformLocation,
};

struct Renderer {
    gl: Gl,
    program: WebGlProgram,
    vertex_buffer: WebGlBuffer,
    matrix_location: WebGlUniformLocation,
    vertex_count: i32,
    camera_angle: f32,
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
    let vertex_buffer = gl
        .create_buffer()
        .ok_or_else(|| JsValue::from_str("Could not allocate the terrain buffer"))?;
    let matrix_location = gl
        .get_uniform_location(&program, "u_view_projection")
        .ok_or_else(|| JsValue::from_str("Camera matrix uniform is missing"))?;

    let vertices = build_terrain_mesh();
    let vertex_count = (vertices.len() / 6) as i32;
    let vertex_data = Float32Array::from(vertices.as_slice());

    gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&vertex_buffer));
    gl.buffer_data_with_array_buffer_view(
        Gl::ARRAY_BUFFER,
        &vertex_data,
        Gl::STATIC_DRAW,
    );

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
            vertex_buffer,
            matrix_location,
            vertex_count,
            camera_angle: 0.0,
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

/// Render one frame from Rust, with a gently orbiting perspective camera.
pub fn draw_frame() -> Result<(), JsValue> {
    RENDERER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let renderer = borrowed
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Renderer has not been started"))?;

        renderer.camera_angle += 0.004;
        let canvas: HtmlCanvasElement = renderer.gl.canvas()
            .ok_or_else(|| JsValue::from_str("WebGL canvas is unavailable"))?
            .dyn_into()
            .map_err(|_| JsValue::from_str("WebGL canvas is not an HTML canvas"))?;
        let width = canvas.width().max(1) as f32;
        let height = canvas.height().max(1) as f32;

        let eye = [
            renderer.camera_angle.sin() * 5.8,
            3.8,
            renderer.camera_angle.cos() * 5.8,
        ];
        let view = look_at(eye, [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
        let projection = perspective(55.0_f32.to_radians(), width / height, 0.1, 100.0);
        let view_projection = multiply(projection, view);

        renderer.gl.use_program(Some(&renderer.program));
        renderer.gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&renderer.vertex_buffer));
        renderer.gl.uniform_matrix4fv_with_f32_array(
            Some(&renderer.matrix_location),
            false,
            &view_projection,
        );
        renderer.gl.clear(Gl::COLOR_BUFFER_BIT | Gl::DEPTH_BUFFER_BIT);
        renderer.gl.draw_arrays(Gl::TRIANGLES, 0, renderer.vertex_count);
        Ok(())
    })
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
fn build_terrain_mesh() -> Vec<f32> {
    let mut world = World::new();
    const HEIGHTS: [[i32; 3]; 3] = [[1, 2, 1], [2, 3, 2], [1, 2, 1]];

    for x in 0..3 {
        for z in 0..3 {
            for y in 0..HEIGHTS[x][z] {
                world.set_block(
                    BlockPosition::new(x as i32, y, z as i32),
                    BlockStateId(1),
                );
            }
        }
    }

    build_world_section_mesh(&world, SectionPosition::new(0, 0, 0))
}

/// Mesh one section while checking neighbouring blocks in the complete world.
/// This suppresses shared faces even when the neighbour is in another section.
fn build_world_section_mesh(world: &World, section_pos: SectionPosition) -> Vec<f32> {
    let mut vertices = Vec::new();

    for y in 0..16_u8 {
        for z in 0..16_u8 {
            for x in 0..16_u8 {
                let local = LocalBlockPosition { x, y, z };
                let world_pos = BlockPosition::new(
                    section_pos.x * 16 + i32::from(x),
                    section_pos.y * 16 + i32::from(y),
                    section_pos.z * 16 + i32::from(z),
                );
                if world.get_block(world_pos) == BlockStateId(0) {
                    continue;
                }

                // Keep the prototype sample centred around the camera.
                let x0 = world_pos.x as f32 - 1.5;
                let x1 = x0 + 1.0;
                let y0 = world_pos.y as f32;
                let y1 = y0 + 1.0;
                let z0 = world_pos.z as f32 - 1.5;
                let z1 = z0 + 1.0;

                let top = [0.28, 0.68, 0.22];
                let side_a = [0.48, 0.32, 0.18];
                let side_b = [0.38, 0.25, 0.14];
                let side_c = [0.54, 0.36, 0.20];
                let bottom = [0.32, 0.22, 0.13];

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
    use super::{build_terrain_mesh, build_world_section_mesh};
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
    fn empty_section_produces_no_geometry() {
        let world = World::new();
        let mesh = build_world_section_mesh(&world, SectionPosition::new(0, 0, 0));
        assert!(mesh.is_empty());
    }
}
