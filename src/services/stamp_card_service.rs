use std::io::Cursor;
use std::sync::{Arc, LazyLock};

use ab_glyph::{FontRef, PxScale};
use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_circle_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::geometric_transformations::{Interpolation, rotate_about_center};
use imageproc::rect::Rect;

const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSansJP-Bold.ttf");

static FONT: LazyLock<FontRef<'static>> =
    LazyLock::new(|| FontRef::try_from_slice(FONT_BYTES).expect("bundled font must be valid"));

const COLUMNS: i32 = 3;
const CELL_WIDTH: i32 = 160;
const CELL_HEIGHT: i32 = 100;
const PADDING: i32 = 20;
const MAX_NAME_CHARS: usize = 6;

const TITLE_AREA_HEIGHT: i32 = 60;
const BORDER_MARGIN: i32 = 10;
const BORDER_GAP: i32 = 6;
const CARD_TITLE: &str = "スタンプカード";

const CARD_BACKGROUND: Rgba<u8> = Rgba([0xFB, 0xF3, 0xE7, 255]);
const CARD_BORDER_COLOR: Rgba<u8> = Rgba([0xB5, 0x4B, 0x3A, 255]);

const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);
const STAMP_COLOR: Rgba<u8> = Rgba([0xB5, 0x4B, 0x3A, 255]);
const EMPTY_BORDER: Rgba<u8> = Rgba([0xE2, 0xE4, 0xE9, 255]);

const STAMP_BUFFER_SIZE: u32 = 96;

const OUTER_RING_OUTER_RADIUS: i32 = 44;
const OUTER_RING_INNER_RADIUS: i32 = 40;
const INNER_RING_OUTER_RADIUS: i32 = 35;
const INNER_RING_INNER_RADIUS: i32 = 33;

const EMPTY_RING_OUTER_RADIUS: i32 = 43;
const EMPTY_RING_INNER_RADIUS: i32 = 41;
const CUSTOM_STAMP_RADIUS: i32 = 42;
const BOLD_OFFSETS: [(i32, i32); 5] = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)];

pub struct StampCell {
    pub label: String,
    pub custom_image: Option<Arc<DynamicImage>>,
}

pub fn render_png(
    stamps: &[StampCell],
    total_rooms: i64,
    card_background: Option<&DynamicImage>,
) -> Vec<u8> {
    let total_rooms = total_rooms.max(1) as i32;
    let rows = (total_rooms + COLUMNS - 1) / COLUMNS;
    let width = (COLUMNS * CELL_WIDTH + PADDING * 2) as u32;
    let height = (TITLE_AREA_HEIGHT + rows * CELL_HEIGHT + PADDING * 2) as u32;

    let mut image: RgbaImage = match card_background {
        Some(background) => background
            .resize_to_fill(width, height, image::imageops::FilterType::Lanczos3)
            .to_rgba8(),
        None => ImageBuffer::from_pixel(width, height, CARD_BACKGROUND),
    };

    if card_background.is_none() {
        draw_card_frame(&mut image, width, height);
        draw_card_title(&mut image, width);
    }

    for i in 0..total_rooms {
        let col = i % COLUMNS;
        let row = i / COLUMNS;
        let center_x = PADDING + col * CELL_WIDTH + CELL_WIDTH / 2;
        let center_y = TITLE_AREA_HEIGHT + PADDING + row * CELL_HEIGHT + CELL_HEIGHT / 2;

        match stamps.get(i as usize) {
            Some(StampCell {
                custom_image: Some(custom),
                ..
            }) => draw_custom_stamp(&mut image, center_x, center_y, custom),
            Some(StampCell { label, .. }) => draw_stamp(&mut image, center_x, center_y, label),
            None => draw_empty_ring(&mut image, center_x, center_y),
        }
    }

    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("PNG encoding should not fail");
    output.into_inner()
}

fn draw_custom_stamp(image: &mut RgbaImage, center_x: i32, center_y: i32, custom: &DynamicImage) {
    let diameter = (CUSTOM_STAMP_RADIUS * 2) as u32;
    let mut cropped = custom
        .resize_to_fill(diameter, diameter, image::imageops::FilterType::Lanczos3)
        .to_rgba8();

    let center = CUSTOM_STAMP_RADIUS;
    for y in 0..diameter as i32 {
        for x in 0..diameter as i32 {
            let dx = x - center;
            let dy = y - center;
            if dx * dx + dy * dy > CUSTOM_STAMP_RADIUS * CUSTOM_STAMP_RADIUS {
                cropped.put_pixel(x as u32, y as u32, TRANSPARENT);
            }
        }
    }

    let offset_x = (center_x - CUSTOM_STAMP_RADIUS) as i64;
    let offset_y = (center_y - CUSTOM_STAMP_RADIUS) as i64;
    image::imageops::overlay(image, &cropped, offset_x, offset_y);
}

fn draw_card_frame(image: &mut RgbaImage, width: u32, height: u32) {
    let outer = Rect::at(BORDER_MARGIN, BORDER_MARGIN).of_size(
        width - (BORDER_MARGIN * 2) as u32,
        height - (BORDER_MARGIN * 2) as u32,
    );
    draw_hollow_rect_mut(image, outer, CARD_BORDER_COLOR);

    let inner_margin = BORDER_MARGIN + BORDER_GAP;
    let inner = Rect::at(inner_margin, inner_margin).of_size(
        width - (inner_margin * 2) as u32,
        height - (inner_margin * 2) as u32,
    );
    draw_hollow_rect_mut(image, inner, CARD_BORDER_COLOR);
}

fn draw_card_title(image: &mut RgbaImage, width: u32) {
    let scale = PxScale::from(30.0);
    let approx_width = CARD_TITLE.chars().count() as i32 * scale.x as i32;
    let x = (width as i32 - approx_width) / 2;
    draw_bold_text_mut(image, CARD_BORDER_COLOR, x, 16, scale, &FONT, CARD_TITLE);
}

#[allow(clippy::too_many_arguments)]
fn draw_bold_text_mut(
    image: &mut RgbaImage,
    color: Rgba<u8>,
    x: i32,
    y: i32,
    scale: PxScale,
    font: &FontRef<'_>,
    text: &str,
) {
    for (dx, dy) in BOLD_OFFSETS {
        draw_text_mut(image, color, x + dx, y + dy, scale, font, text);
    }
}

fn draw_empty_ring(image: &mut RgbaImage, center_x: i32, center_y: i32) {
    draw_filled_circle_mut(
        image,
        (center_x, center_y),
        EMPTY_RING_OUTER_RADIUS,
        EMPTY_BORDER,
    );
    draw_filled_circle_mut(
        image,
        (center_x, center_y),
        EMPTY_RING_INNER_RADIUS,
        CARD_BACKGROUND,
    );
}

fn draw_stamp(image: &mut RgbaImage, center_x: i32, center_y: i32, name: &str) {
    let buffer_center = (STAMP_BUFFER_SIZE / 2) as i32;
    let mut buffer: RgbaImage =
        ImageBuffer::from_pixel(STAMP_BUFFER_SIZE, STAMP_BUFFER_SIZE, TRANSPARENT);

    draw_stamp_rings(image, center_x, center_y);

    let label = truncate_room_name(name);
    let lines = split_stamp_label_lines(&label);
    let scale = PxScale::from(22.0);
    let line_height = 22;
    let start_y = buffer_center - (lines.len() as i32 * line_height) / 2;
    for (idx, line) in lines.iter().enumerate() {
        let approx_width = line.chars().count() as i32 * scale.x as i32;
        draw_bold_text_mut(
            &mut buffer,
            STAMP_COLOR,
            buffer_center - approx_width / 2,
            start_y + idx as i32 * line_height,
            scale,
            &FONT,
            line,
        );
    }

    let theta = stamp_rotation_degrees(name).to_radians();
    let rotated = rotate_about_center(&buffer, theta, Interpolation::Bilinear, TRANSPARENT);

    let offset_x = (center_x - buffer_center) as i64;
    let offset_y = (center_y - buffer_center) as i64;
    image::imageops::overlay(image, &rotated, offset_x, offset_y);
}

fn draw_stamp_rings(image: &mut RgbaImage, center_x: i32, center_y: i32) {
    draw_filled_circle_mut(
        image,
        (center_x, center_y),
        OUTER_RING_OUTER_RADIUS,
        STAMP_COLOR,
    );
    draw_filled_circle_mut(
        image,
        (center_x, center_y),
        OUTER_RING_INNER_RADIUS,
        CARD_BACKGROUND,
    );
    draw_filled_circle_mut(
        image,
        (center_x, center_y),
        INNER_RING_OUTER_RADIUS,
        STAMP_COLOR,
    );
    draw_filled_circle_mut(
        image,
        (center_x, center_y),
        INNER_RING_INNER_RADIUS,
        CARD_BACKGROUND,
    );
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

fn split_stamp_label_lines(label: &str) -> Vec<String> {
    let chars: Vec<char> = label.chars().collect();
    if chars.len() <= 3 {
        return vec![label.to_string()];
    }
    let half = (chars.len() + 2) / 2;
    vec![
        chars[..half].iter().collect(),
        chars[half..].iter().collect(),
    ]
}

fn stamp_rotation_degrees(name: &str) -> f32 {
    let sum: u32 = name.bytes().map(u32::from).sum();
    (sum % 17) as f32 - 8.0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use image::{DynamicImage, GenericImageView, ImageBuffer, ImageFormat, Rgba};

    const STAMP_COLOR: Rgba<u8> = Rgba([0xB5, 0x4B, 0x3A, 255]);
    const CARD_BACKGROUND: Rgba<u8> = Rgba([0xFB, 0xF3, 0xE7, 255]);
    const EMPTY_BORDER: Rgba<u8> = Rgba([0xE2, 0xE4, 0xE9, 255]);
    const CUSTOM_STAMP_COLOR: Rgba<u8> = Rgba([0x21, 0x9E, 0xBC, 255]);
    const CUSTOM_BACKGROUND_COLOR: Rgba<u8> = Rgba([0x24, 0x6A, 0x73, 255]);

    fn stamp(label: &str) -> super::StampCell {
        super::StampCell {
            label: label.to_string(),
            custom_image: None,
        }
    }

    fn solid_image(color: Rgba<u8>, width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(ImageBuffer::from_pixel(width, height, color))
    }

    #[test]
    fn render_empty_card_returns_png_with_expected_dimensions() {
        let png = super::render_png(&[], 15, None);

        assert_eq!(image::guess_format(&png).unwrap(), ImageFormat::Png);
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!(image.dimensions(), (520, 567));
    }

    #[test]
    fn stamped_first_cell_has_outer_ring_at_top() {
        let png = super::render_png(&[stamp("図書室")], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(120, 103), STAMP_COLOR);
    }

    #[test]
    fn stamped_first_cell_has_inner_ring_at_top() {
        let png = super::render_png(&[stamp("図書室")], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(120, 111), STAMP_COLOR);
    }

    #[test]
    fn stamped_first_cell_keeps_gap_between_rings_unfilled() {
        let png = super::render_png(&[stamp("図書室")], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(120, 108), CARD_BACKGROUND);
    }

    #[test]
    fn empty_first_cell_has_ring_outline_at_top() {
        let png = super::render_png(&[], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(120, 103), EMPTY_BORDER);
    }

    #[test]
    fn empty_first_cell_center_remains_card_background() {
        let png = super::render_png(&[], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(120, 145), CARD_BACKGROUND);
    }

    #[test]
    fn stamped_cells_are_ringed_in_visit_order() {
        let png = super::render_png(&[stamp("A"), stamp("B"), stamp("C")], 5, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(400, 103), STAMP_COLOR);
        assert_eq!(*image.get_pixel(120, 192), EMPTY_BORDER);
    }

    #[test]
    fn zero_total_rooms_renders_one_cell_without_panicking() {
        let png = super::render_png(&[], 0, None);

        assert_eq!(image::guess_format(&png).unwrap(), ImageFormat::Png);
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!(image.dimensions(), (520, 211));
    }

    #[test]
    fn split_stamp_label_lines_keeps_short_labels_on_one_line() {
        assert_eq!(super::split_stamp_label_lines("図書室"), vec!["図書室"]);
    }

    #[test]
    fn split_stamp_label_lines_splits_long_labels_with_more_chars_on_first_line() {
        assert_eq!(
            super::split_stamp_label_lines("とても長い…"),
            vec!["とても長", "い…"]
        );
    }

    #[test]
    fn stamp_rotation_degrees_is_deterministic_and_bounded() {
        let first = super::stamp_rotation_degrees("図書室");
        let second = super::stamp_rotation_degrees("図書室");

        assert_eq!(first, second);
        assert!((-8.0..=8.0).contains(&first));
    }

    #[test]
    fn title_area_point_away_from_text_and_frame_is_card_background() {
        let png = super::render_png(&[], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(40, 40), CARD_BACKGROUND);
    }

    #[test]
    fn outer_card_frame_is_stamp_color() {
        let png = super::render_png(&[], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(260, 10), STAMP_COLOR);
    }

    #[test]
    fn inner_card_frame_is_stamp_color() {
        let png = super::render_png(&[], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(260, 16), STAMP_COLOR);
    }

    #[test]
    fn custom_stamp_image_replaces_generated_stamp_at_cell_center() {
        let custom = Arc::new(solid_image(CUSTOM_STAMP_COLOR, 96, 96));
        let png = super::render_png(
            &[super::StampCell {
                label: "図書室".to_string(),
                custom_image: Some(custom),
            }],
            15,
            None,
        );
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(120, 145), CUSTOM_STAMP_COLOR);
    }

    #[test]
    fn stamp_without_custom_image_keeps_generated_stamp_ring() {
        let png = super::render_png(&[stamp("図書室")], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(120, 103), STAMP_COLOR);
    }

    #[test]
    fn custom_card_background_replaces_default_background() {
        let background = solid_image(CUSTOM_BACKGROUND_COLOR, 520, 600);
        let png = super::render_png(&[], 15, Some(&background));
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(40, 40), CUSTOM_BACKGROUND_COLOR);
    }

    #[test]
    fn custom_card_background_keeps_frame_pixels_from_background_image() {
        let background = solid_image(CUSTOM_BACKGROUND_COLOR, 520, 600);
        let png = super::render_png(&[], 15, Some(&background));
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(260, 10), CUSTOM_BACKGROUND_COLOR);
        assert_eq!(*image.get_pixel(260, 16), CUSTOM_BACKGROUND_COLOR);
    }

    #[test]
    fn custom_card_background_keeps_title_area_pixels_from_background_image() {
        let background = solid_image(CUSTOM_BACKGROUND_COLOR, 520, 600);
        let png = super::render_png(&[], 15, Some(&background));
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        for y in 16..52 {
            for x in 150..370 {
                assert_eq!(*image.get_pixel(x, y), CUSTOM_BACKGROUND_COLOR);
            }
        }
    }

    #[test]
    fn missing_card_background_keeps_default_background() {
        let png = super::render_png(&[], 15, None);
        let image = image::load_from_memory(&png).unwrap().to_rgba8();

        assert_eq!(*image.get_pixel(40, 40), CARD_BACKGROUND);
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
