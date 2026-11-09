//! Instance covers: one local PNG, decoded to RGBA8 and resampled to the handful
//! of pixels a terminal can show.
//!
//! A 120-column terminal gives the cover column about 34 cells, and a cell
//! holds two square pixels through `▀`, so the picture is 34×34 in the best
//! case. Everything here is therefore about *sampling*, not scaling: the
//! source is area-averaged down to the exact target, so a 128×128 world
//! screenshot keeps its average colour instead of dropping rows and columns.
//!
//! The decoder lives in `core`, not in `tui`, because the pixels have nothing
//! to do with Ratatui; the widget only turns them into half blocks.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::core::error::{Error, Result};

/// Covers are icons, not wallpapers. Refusing anything larger keeps a hostile
/// or accidentally huge PNG from allocating hundreds of megabytes before the
/// first pixel is drawn.
const MAX_SOURCE_EDGE: u32 = 2048;

/// An 8-bit RGBA image, row-major, four bytes per pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Cover {
    /// Builds a cover from raw RGBA bytes.
    ///
    /// The length check is the only validation: callers either decoded the
    /// bytes themselves or are tests building a known image.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Error::Cover(format!(
                "empty cover dimensions {width}x{height}"
            )));
        }
        if width > MAX_SOURCE_EDGE || height > MAX_SOURCE_EDGE {
            return Err(Error::Cover(format!(
                "cover dimensions {width}x{height} exceed {MAX_SOURCE_EDGE}"
            )));
        }
        let expected = u64::from(width) * u64::from(height) * 4;
        if u64::try_from(rgba.len()).unwrap_or(u64::MAX) != expected {
            return Err(Error::Cover(format!(
                "cover buffer is {} bytes, expected {expected} for {width}x{height}",
                rgba.len()
            )));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Decodes a PNG into RGBA8.
    ///
    /// Palette images and `tRNS` transparency are expanded to alpha, and
    /// 16-bit samples are reduced to their high byte, so the rest of the code
    /// only ever sees the four colour types handled below.
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mut decoder = png::Decoder::new(BufReader::new(file));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder
            .read_info()
            .map_err(|error| decode_error(path, error))?;
        let width = reader.info().width;
        let height = reader.info().height;
        if width == 0 || height == 0 || width > MAX_SOURCE_EDGE || height > MAX_SOURCE_EDGE {
            return Err(Error::Cover(format!(
                "{}: unsupported dimensions {width}x{height}",
                path.display()
            )));
        }

        let mut buffer = vec![0; reader.output_buffer_size()];
        let frame = reader
            .next_frame(&mut buffer)
            .map_err(|error| decode_error(path, error))?;
        let data = &buffer[..frame.buffer_size()];
        let rgba = to_rgba(data, frame.color_type).ok_or_else(|| {
            Error::Cover(format!(
                "{}: unexpected {:?} pixel buffer",
                path.display(),
                frame.color_type
            ))
        })?;
        Self::from_rgba(frame.width, frame.height, rgba)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Reads one pixel; out-of-range coordinates are transparent black so a
    /// painter can ask for the border without branching on every cell.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let index = ((u64::from(y) * u64::from(self.width) + u64::from(x)) * 4) as usize;
        [
            self.rgba[index],
            self.rgba[index + 1],
            self.rgba[index + 2],
            self.rgba[index + 3],
        ]
    }

    /// Centre-crops to a square. Covers are shown square because a terminal
    /// cell is twice as tall as it is wide, and `▀` is what makes that square;
    /// letterboxing a 16:9 image into a square would waste the few pixels we
    /// have.
    pub fn square(&self) -> Self {
        let side = self.width.min(self.height);
        if side == self.width && side == self.height {
            return self.clone();
        }
        let left = (self.width - side) / 2;
        let top = (self.height - side) / 2;
        let mut rgba = Vec::with_capacity((u64::from(side) * u64::from(side) * 4) as usize);
        for row in 0..side {
            let start =
                ((u64::from(top + row) * u64::from(self.width) + u64::from(left)) * 4) as usize;
            let end = start + (side as usize) * 4;
            rgba.extend_from_slice(&self.rgba[start..end]);
        }
        Self {
            width: side,
            height: side,
            rgba,
        }
    }

    /// Area-averaged resample to exactly `width`×`height`.
    ///
    /// Averaging is done in premultiplied alpha so a transparent pixel cannot
    /// darken the colour next to it; a half-transparent red and an opaque red
    /// must both resample to red, not to a muddy maroon.
    pub fn resized(&self, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return self.clone();
        }
        let mut rgba = vec![0; (u64::from(width) * u64::from(height) * 4) as usize];
        for y in 0..height {
            let (y0, y1) = span(y, self.height, height);
            let y1 = y1.max(y0 + 1).min(self.height);
            for x in 0..width {
                let (x0, x1) = span(x, self.width, width);
                let x1 = x1.max(x0 + 1).min(self.width);
                let (mut r, mut g, mut b, mut a) = (0u64, 0u64, 0u64, 0u64);
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let pixel = self.pixel(sx, sy);
                        let alpha = u64::from(pixel[3]);
                        r += u64::from(pixel[0]) * alpha;
                        g += u64::from(pixel[1]) * alpha;
                        b += u64::from(pixel[2]) * alpha;
                        a += alpha;
                    }
                }
                let count = u64::from(x1 - x0) * u64::from(y1 - y0);
                let index = ((u64::from(y) * u64::from(width) + u64::from(x)) * 4) as usize;
                // `a` is the sum of alpha, so a fully transparent box divides
                // by zero; `checked_div` keeps that case at black-transparent
                // without a branch that clippy reads as a manual check.
                rgba[index] = (r + a / 2).checked_div(a).unwrap_or(0) as u8;
                rgba[index + 1] = (g + a / 2).checked_div(a).unwrap_or(0) as u8;
                rgba[index + 2] = (b + a / 2).checked_div(a).unwrap_or(0) as u8;
                rgba[index + 3] = ((a + count / 2) / count) as u8;
            }
        }
        Self {
            width,
            height,
            rgba,
        }
    }
}

/// Source range covered by one destination pixel.
///
/// Boundaries are rounded, not floored/ceiled: reducing five pixels to two
/// must split them `0..3` and `3..5`, not `0..3` and `2..5`. The latter counts
/// the middle pixel twice and gives it twice the weight it deserves.
fn span(index: u32, source: u32, target: u32) -> (u32, u32) {
    let boundary = |edge: u32| -> u32 {
        ((u64::from(edge) * u64::from(source) + u64::from(target) / 2) / u64::from(target)) as u32
    };
    let start = boundary(index).min(source.saturating_sub(1));
    (start, boundary(index + 1).min(source))
}

fn decode_error(path: &Path, error: impl std::fmt::Display) -> Error {
    Error::Cover(format!("{}: {error}", path.display()))
}

fn to_rgba(data: &[u8], color_type: png::ColorType) -> Option<Vec<u8>> {
    let mut rgba = Vec::with_capacity(data.len() / 3 * 4);
    match color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(data),
        png::ColorType::Rgb => {
            for chunk in data.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &gray in data {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for chunk in data.chunks_exact(2) {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
        }
        // `normalize_to_color8` expands indexed images, so this is defensive.
        png::ColorType::Indexed => return None,
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::Cover;
    use crate::core::error::Error;
    use std::path::PathBuf;

    fn cover(width: u32, height: u32, pixels: &[[u8; 4]]) -> Cover {
        let mut rgba = Vec::with_capacity(pixels.len() * 4);
        for pixel in pixels {
            rgba.extend_from_slice(pixel);
        }
        Cover::from_rgba(width, height, rgba).expect("cover")
    }

    fn temp_png(tag: &str, bytes: &[u8]) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tmcl-cover-{tag}-{}.png", uuid::Uuid::new_v4()));
        std::fs::write(&path, bytes).expect("write png");
        path
    }

    fn encode(
        width: u32,
        height: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        data: &[u8],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(color);
            encoder.set_depth(depth);
            let mut writer = encoder.write_header().expect("png header");
            writer.write_image_data(data).expect("png data");
        }
        bytes
    }

    #[test]
    fn rejects_buffers_that_do_not_match_their_dimensions() {
        assert!(matches!(
            Cover::from_rgba(2, 2, vec![0; 15]),
            Err(Error::Cover(_))
        ));
        assert!(matches!(
            Cover::from_rgba(0, 2, Vec::new()),
            Err(Error::Cover(_))
        ));
    }

    #[test]
    fn resize_averages_a_checkerboard_instead_of_dropping_pixels() {
        let source = cover(
            2,
            2,
            &[
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                [0, 0, 255, 255],
                [255, 0, 0, 255],
            ],
        );
        let one = source.resized(1, 1);
        assert_eq!(one.pixel(0, 0), [128, 0, 128, 255]);
    }

    #[test]
    fn resize_keeps_colour_when_alpha_covers_only_half_the_pixels() {
        let source = cover(2, 1, &[[255, 0, 0, 255], [0, 0, 255, 0]]);
        let one = source.resized(1, 1);
        assert_eq!(one.pixel(0, 0), [255, 0, 0, 128]);
    }

    #[test]
    fn resize_partitions_a_non_divisible_source_without_double_counting() {
        let source = cover(
            5,
            1,
            &[
                [0, 0, 0, 255],
                [10, 10, 10, 255],
                [20, 20, 20, 255],
                [30, 30, 30, 255],
                [40, 40, 40, 255],
            ],
        );
        let halves = source.resized(2, 1);
        assert_eq!(halves.pixel(0, 0), [10, 10, 10, 255]);
        assert_eq!(halves.pixel(1, 0), [35, 35, 35, 255]);
    }

    #[test]
    fn square_crops_the_centre_column() {
        let source = cover(
            3,
            1,
            &[[0, 0, 0, 255], [90, 90, 90, 255], [255, 255, 255, 255]],
        );
        let one = source.square();
        assert_eq!((one.width(), one.height()), (1, 1));
        assert_eq!(one.pixel(0, 0), [90, 90, 90, 255]);
    }

    #[test]
    fn load_reads_rgb_and_adds_an_opaque_alpha() {
        let path = temp_png(
            "rgb",
            &encode(
                2,
                1,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &[255, 0, 0, 0, 255, 0],
            ),
        );
        let loaded = Cover::load(&path).expect("decode rgb");
        assert_eq!(loaded.width(), 2);
        assert_eq!(loaded.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(loaded.pixel(1, 0), [0, 255, 0, 255]);
        std::fs::remove_file(path).expect("remove png");
    }

    #[test]
    fn load_reads_rgba_with_transparency() {
        let path = temp_png(
            "rgba",
            &encode(
                1,
                1,
                png::ColorType::Rgba,
                png::BitDepth::Eight,
                &[10, 20, 30, 40],
            ),
        );
        let loaded = Cover::load(&path).expect("decode rgba");
        assert_eq!(loaded.pixel(0, 0), [10, 20, 30, 40]);
        std::fs::remove_file(path).expect("remove png");
    }

    #[test]
    fn load_reads_grayscale_and_grayscale_alpha() {
        let gray = temp_png(
            "gray",
            &encode(
                2,
                1,
                png::ColorType::Grayscale,
                png::BitDepth::Eight,
                &[0, 255],
            ),
        );
        let loaded = Cover::load(&gray).expect("decode gray");
        assert_eq!(loaded.pixel(0, 0), [0, 0, 0, 255]);
        assert_eq!(loaded.pixel(1, 0), [255, 255, 255, 255]);
        std::fs::remove_file(gray).expect("remove png");

        let alpha = temp_png(
            "gray-alpha",
            &encode(
                1,
                1,
                png::ColorType::GrayscaleAlpha,
                png::BitDepth::Eight,
                &[10, 20],
            ),
        );
        let loaded = Cover::load(&alpha).expect("decode gray alpha");
        assert_eq!(loaded.pixel(0, 0), [10, 10, 10, 20]);
        std::fs::remove_file(alpha).expect("remove png");
    }

    #[test]
    fn load_expands_a_palette_to_rgb() {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![255, 0, 0, 0, 0, 255]);
            let mut writer = encoder.write_header().expect("png header");
            writer.write_image_data(&[0, 1]).expect("png data");
        }
        let path = temp_png("palette", &bytes);
        let loaded = Cover::load(&path).expect("decode palette");
        assert_eq!(loaded.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(loaded.pixel(1, 0), [0, 0, 255, 255]);
        std::fs::remove_file(path).expect("remove png");
    }

    #[test]
    fn load_keeps_the_high_byte_of_a_sixteen_bit_png() {
        let path = temp_png(
            "rgb16",
            &encode(
                1,
                1,
                png::ColorType::Rgb,
                png::BitDepth::Sixteen,
                &[0xFF, 0x00, 0x80, 0x00, 0x12, 0x34],
            ),
        );
        let loaded = Cover::load(&path).expect("decode 16-bit");
        assert_eq!(loaded.pixel(0, 0), [255, 128, 0x12, 255]);
        std::fs::remove_file(path).expect("remove png");
    }

    #[test]
    fn load_rejects_a_file_that_is_not_a_png() {
        let path = temp_png("bogus", b"not a png");
        assert!(matches!(Cover::load(&path), Err(Error::Cover(_))));
        std::fs::remove_file(path).expect("remove file");
    }

    #[test]
    fn load_rejects_an_image_wider_than_the_limit() {
        let path = temp_png(
            "wide",
            &encode(
                super::MAX_SOURCE_EDGE + 1,
                1,
                png::ColorType::Rgb,
                png::BitDepth::Eight,
                &vec![0; ((super::MAX_SOURCE_EDGE + 1) * 3) as usize],
            ),
        );
        assert!(matches!(Cover::load(&path), Err(Error::Cover(_))));
        std::fs::remove_file(path).expect("remove png");
    }
}
