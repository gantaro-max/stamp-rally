pub const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum ImageError {
    TooLarge,
    InvalidFormat,
}

pub fn process_upload(bytes: &[u8]) -> Result<Vec<u8>, ImageError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ImageError::TooLarge);
    }

    match image::guess_format(bytes) {
        Ok(image::ImageFormat::Jpeg | image::ImageFormat::Png) => Ok(Vec::new()),
        _ => Err(ImageError::InvalidFormat),
    }
}

#[cfg(test)]
mod tests {
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
}
