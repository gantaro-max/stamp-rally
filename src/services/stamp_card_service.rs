use std::io::Cursor;
use std::sync::LazyLock;

use ab_glyph::{FontRef, PxScale};
use image::{ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSansJP-Bold.ttf");

static FONT: LazyLock<FontRef<'static>> =
    LazyLock::new(|| FontRef::try_from_slice(FONT_BYTES).expect("bundled font must be valid"));

const COLUMNS: i32 = 3;
const CELL_WIDTH: i32 = 160;
const CELL_HEIGHT: i32 = 100;
const PADDING: i32 = 20;
const CELL_MARGIN: i32 = 8;
const MAX_NAME_CHARS: usize = 6;

const BACKGROUND: Rgba<u8> = Rgba([255, 255, 255, 255]);
const STAMPED_FILL: Rgba<u8> = Rgba([0xB5, 0x4B, 0x3A, 255]);
const STAMPED_TEXT: Rgba<u8> = Rgba([255, 255, 255, 255]);
const EMPTY_BORDER: Rgba<u8> = Rgba([0xE2, 0xE4, 0xE9, 255]);

pub fn render_png(room_names: &[String], total_rooms: i64) -> Vec<u8> {
    let total_rooms = total_rooms.max(1) as i32;
    let rows = (total_rooms + COLUMNS - 1) / COLUMNS;
    let width = (COLUMNS * CELL_WIDTH + PADDING * 2) as u32;
    let height = (rows * CELL_HEIGHT + PADDING * 2) as u32;

    let mut image: RgbaImage = ImageBuffer::from_pixel(width, height, BACKGROUND);
    let scale = PxScale::from(24.0);

    for i in 0..total_rooms {
        let col = i % COLUMNS;
        let row = i / COLUMNS;
        let x = PADDING + col * CELL_WIDTH + CELL_MARGIN;
        let y = PADDING + row * CELL_HEIGHT + CELL_MARGIN;
        let rect_width = (CELL_WIDTH - CELL_MARGIN * 2) as u32;
        let rect_height = (CELL_HEIGHT - CELL_MARGIN * 2) as u32;
        let rect = Rect::at(x, y).of_size(rect_width, rect_height);

        if let Some(name) = room_names.get(i as usize) {
            draw_filled_rect_mut(&mut image, rect, STAMPED_FILL);
            let label = truncate_room_name(name);
            draw_text_mut(
                &mut image,
                STAMPED_TEXT,
                x + 10,
                y + rect_height as i32 / 2 - 12,
                scale,
                &*FONT,
                &label,
            );
        } else {
            draw_hollow_rect_mut(&mut image, rect, EMPTY_BORDER);
        }
    }

    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("PNG encoding should not fail");
    output.into_inner()
}

fn truncate_room_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= MAX_NAME_CHARS {
        return name.to_string();
    }
    let mut truncated: String = chars[..MAX_NAME_CHARS].iter().collect();
    truncated.push('…');
    truncated
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
