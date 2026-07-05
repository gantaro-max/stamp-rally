use std::io::Cursor;

use qrcode::QrCode;

pub fn render_png(value: &str) -> Vec<u8> {
    let code = QrCode::new(value.as_bytes()).expect("QR code generation should not fail");
    let image = code.render::<image::Luma<u8>>().build();
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("PNG encoding should not fail");
    output.into_inner()
}

#[cfg(test)]
mod tests {
    use super::render_png;

    #[test]
    fn renders_png_qr_code_with_original_value() {
        let value = "room-qr-uuid";

        let png = render_png(value);

        assert_eq!(image::guess_format(&png).unwrap(), image::ImageFormat::Png);
        let image = image::load_from_memory(&png).unwrap().to_luma8();
        let mut prepared = rqrr::PreparedImage::prepare(image);
        let grids = prepared.detect_grids();
        let (_, decoded) = grids[0].decode().unwrap();

        assert_eq!(decoded, value);
    }
}
