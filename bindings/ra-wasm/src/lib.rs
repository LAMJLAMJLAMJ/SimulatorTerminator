//! The browser wrapper. It moves JSON commands and byte buffers between a
//! worker and [`ra_api::Engine`], and does nothing else.

use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmEngine {
    inner: ra_api::Engine,
}

#[wasm_bindgen]
impl WasmEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmEngine {
        WasmEngine { inner: ra_api::Engine::new() }
    }

    /// `cmd` is a JSON `Command`, `bufs` an array of `Uint8Array`, `on_event` is called with
    /// each progress event as JSON. Returns `{ event: string, bufs: Uint8Array[] }`.
    pub fn handle(&mut self, cmd: &str, bufs: Array, on_event: Option<Function>) -> Result<Object, JsValue> {
        let bufs: Vec<Vec<u8>> = bufs.iter().map(|b| Uint8Array::new(&b).to_vec()).collect();
        let mut progress = |e: String| {
            if let Some(f) = &on_event {
                let _ = f.call1(&JsValue::NULL, &JsValue::from_str(&e));
            }
        };
        let (event, out) = self.inner.handle_json(cmd, bufs, &mut progress);
        let out_bufs: Array = out.iter().map(|b| Uint8Array::from(b.as_slice())).collect();
        let reply = Object::new();
        Reflect::set(&reply, &"event".into(), &event.into())?;
        Reflect::set(&reply, &"bufs".into(), &out_bufs)?;
        Ok(reply)
    }
}

impl Default for WasmEngine {
    fn default() -> Self {
        Self::new()
    }
}
