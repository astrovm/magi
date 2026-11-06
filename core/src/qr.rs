use std::fmt::Write;

use qrcodegen::{QrCode, QrCodeEcc};

const PIXEL_SIZE: i32 = 10;
const BORDER: i32 = 2;

/// Draws a QR code as an SVG, or nothing for text too long to fit in one.
pub fn render_svg(text: &str) -> String {
    let Ok(code) = QrCode::encode_text(text, QrCodeEcc::Low) else {
        return String::new();
    };
    let side = (code.size() + BORDER * 2) * PIXEL_SIZE;
    let mut path = String::new();
    for row in 0..code.size() {
        for col in 0..code.size() {
            if code.get_module(col, row) {
                let x = (col + BORDER) * PIXEL_SIZE;
                let y = (row + BORDER) * PIXEL_SIZE;
                // Writing to a String can't fail.
                let _ = write!(path, "M{x},{y}h{PIXEL_SIZE}v{PIXEL_SIZE}h-{PIXEL_SIZE}z");
            }
        }
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {side} {side}"><rect fill="white" width="{side}" height="{side}"/><path fill="black" d="{path}"/></svg>"#
    )
}
