mod chip8;

use chip8::Chip8;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn run() {
    let mut chip8 = Chip8::new();
    chip8.execute_instruction();
}