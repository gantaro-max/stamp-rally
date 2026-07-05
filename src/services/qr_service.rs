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
