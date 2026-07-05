pub const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum ImageError {
    TooLarge,
    InvalidFormat,
    DecodeFailed,
}

pub fn process_upload(bytes: &[u8]) -> Result<Vec<u8>, ImageError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ImageError::TooLarge);
    }

    let format = match image::guess_format(bytes) {
        Ok(image::ImageFormat::Jpeg | image::ImageFormat::Png) => image::guess_format(bytes).unwrap(),
        _ => return Err(ImageError::InvalidFormat),
    };

    let image = image::load_from_memory_with_format(bytes, format)
        .map_err(|_| ImageError::DecodeFailed)?;
    let resized = if image.width() > 800 {
        image.resize(800, u32::MAX, image::imageops::FilterType::Lanczos3)
    } else {
        image
    };
    let rgb = resized.to_rgb8();
    let mut output = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 80);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|_| ImageError::DecodeFailed)?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

    use super::{ImageError, process_upload};

    #[test]
    fn rejects_uploads_larger_than_five_megabytes() {
        let bytes = vec![0_u8; 5 * 1024 * 1024 + 1];

        let err = process_upload(&bytes).unwrap_err();

        assert!(matches!(err, ImageError::TooLarge));
    }

    #[test]
    fn rejects_non_jpeg_or_png_magic_bytes() {
        let err = process_upload(b"not an image").unwrap_err();

        assert!(matches!(err, ImageError::InvalidFormat));
    }

    #[test]
    fn resizes_valid_png_to_jpeg_with_max_width() {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(1200, 600, Rgb([200, 10, 10])));
        let mut input = Cursor::new(Vec::new());
        image.write_to(&mut input, ImageFormat::Png).unwrap();

        let output = process_upload(input.get_ref()).unwrap();

        assert_eq!(image::guess_format(&output).unwrap(), ImageFormat::Jpeg);
        let decoded = image::load_from_memory(&output).unwrap();
        assert!(decoded.width() <= 800);
    }
}
