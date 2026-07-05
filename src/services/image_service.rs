#[cfg(test)]
mod tests {
    use super::{ImageError, process_upload};

    #[test]
    fn rejects_uploads_larger_than_five_megabytes() {
        let bytes = vec![0_u8; 5 * 1024 * 1024 + 1];

        let err = process_upload(&bytes).unwrap_err();

        assert!(matches!(err, ImageError::TooLarge));
    }
}
