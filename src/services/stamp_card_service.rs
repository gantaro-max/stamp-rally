pub fn render_png(_room_names: &[String], _total_rooms: i64) -> Vec<u8> {
    Vec::new()
}

fn truncate_room_name(name: &str) -> String {
    name.to_string()
}

#[cfg(test)]
mod tests {
    use image::{GenericImageView, ImageFormat, Rgba};

    const STAMPED_FILL: Rgba<u8> = Rgba([0xB5, 0x4B, 0x3A, 255]);
    const BACKGROUND: Rgba<u8> = Rgba([255, 255, 255, 255]);

    #[test]
    fn render_empty_card_returns_png_with_expected_dimensions() {
        let png = super::render_png(&[], 15);

        assert_eq!(image::guess_format(&png).unwrap(), ImageFormat::Png);
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!(image.dimensions(), (520, 540));
    }

    #[test]
    fn stamped_first_cell_is_filled_with_stamp_color() {
        let png = super::render_png(&["図書室".to_string()], 15);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(32, 32), STAMPED_FILL);
    }

    #[test]
    fn empty_first_cell_center_remains_background() {
        let png = super::render_png(&[], 15);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(100, 70), BACKGROUND);
    }

    #[test]
    fn stamped_cells_are_filled_in_visit_order() {
        let png = super::render_png(&["A".to_string(), "B".to_string(), "C".to_string()], 5);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(352, 32), STAMPED_FILL);
        assert_eq!(*image.get_pixel(100, 170), BACKGROUND);
    }

    #[test]
    fn zero_total_rooms_renders_one_cell_without_panicking() {
        let png = super::render_png(&[], 0);

        assert_eq!(image::guess_format(&png).unwrap(), ImageFormat::Png);
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!(image.dimensions(), (520, 140));
    }

    #[test]
    fn truncate_room_name_keeps_short_names_and_truncates_long_names() {
        assert_eq!(super::truncate_room_name("図書室"), "図書室");
        assert_eq!(super::truncate_room_name("123456"), "123456");
        assert_eq!(
            super::truncate_room_name("とても長い部屋の名前です"),
            "とても長い部…"
        );
    }
}
