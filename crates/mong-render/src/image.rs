//! PNG → RGBA8. Dùng `png` chứ không `image`: ngân sách bundle wasm 5 MB
//! (DoD M4) không chịu nổi cả cây codec.

use crate::RenderError;

pub fn decode_png(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), RenderError> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder
        .read_info()
        .map_err(|e| RenderError::Decode(e.to_string()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| RenderError::Decode(e.to_string()))?;
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => {
            return Err(RenderError::Decode(format!(
                "color type {other:?} chua ho tro"
            )))
        }
    };
    Ok((rgba, info.width, info.height))
}
