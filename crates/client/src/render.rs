//! First browser renderer: a real WebGL1 context with a tiny frame loop.
//!
//! This is deliberately just the graphics bootstrap. It proves the browser
//! canvas -> Rust/WASM -> WebGL path before we add shaders, meshes and chunks.

use std::cell::RefCell;
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{HtmlCanvasElement, WebGlRenderingContext};

thread_local! {
    static GL: RefCell<Option<WebGlRenderingContext>> = const { RefCell::new(None) };
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

/// Find the canvas, ask the browser for WebGL1, and paint the first frame.
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
    let gl: WebGlRenderingContext = context
        .dyn_into()
        .map_err(|_| JsValue::from_str("Could not create a WebGL1 context"))?;

    GL.with(|slot| *slot.borrow_mut() = Some(gl));
    resize_renderer(canvas_id)?;
    draw_frame()
}

/// Match the drawing buffer to the canvas's displayed size.
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

    GL.with(|slot| {
        if let Some(gl) = slot.borrow().as_ref() {
            gl.viewport(0, 0, width as i32, height as i32);
            Ok(())
        } else {
            Err(JsValue::from_str("Renderer has not been started"))
        }
    })
}

/// Draw one frame. The scene is only a clear colour for now.
pub fn draw_frame() -> Result<(), JsValue> {
    GL.with(|slot| {
        let borrowed = slot.borrow();
        let gl = borrowed
            .as_ref()
            .ok_or_else(|| JsValue::from_str("Renderer has not been started"))?;

        gl.clear_color(0.035, 0.055, 0.09, 1.0);
        gl.clear(WebGlRenderingContext::COLOR_BUFFER_BIT);
        Ok(())
    })
}
