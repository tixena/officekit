use alloc::vec::Vec;
use std::io::Cursor;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use image_webp::{ColorType as WebPColor, WebPEncoder};
use officekit_packaging::image::{Image, ImageFormat};
use os_shim::System as _;
use os_shim::mock::MemorySystem;
use png::{BitDepth, ColorType, Decoder, Encoder};

use resvg::tiny_skia::Size;

use super::{Picture, drawn_size, read};
use crate::markdown::MarkdownError;

/// The header of an AVIF file, a format officekit does not store.
const AVIF: &[u8] = b"\x00\x00\x00\x18ftypavif\x00\x00\x00\x00avifmif1";

/// A BMP header, 3 by 2 pixels.
const BMP: &[u8] = b"BM\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x02\x00\x00\x00";

/// A GIF header, 3 by 2 pixels.
const GIF: &[u8] = b"GIF89a\x03\x00\x02\x00\x00\x00\x00;";

/// A JPEG of a start-of-frame marker, 3 by 2 pixels.
const JPEG: &[u8] = b"\xFF\xD8\xFF\xC0\x00\x0B\x08\x00\x02\x00\x03\x01\x01\x11\x00\xFF\xD9";

/// The SVG of the design's sample: a blue rectangle 120 by 80.
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80"><rect width="120" height="80" fill="#2b6cb0"/></svg>"##;

/// [`SVG`] percent-encoded in a `data:` address.
const SVG_ADDRESS: &str = "data:image/svg+xml,%3Csvg%20xmlns=%22http://www.w3.org/2000/svg%22%20width=%22120%22%20height=%2280%22%3E%3Crect%20width=%22120%22%20height=%2280%22%20fill=%22%232b6cb0%22/%3E%3C/svg%3E";

/// A little-endian TIFF header.
const TIFF: &[u8] = b"II*\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

/// A stored picture's image, its SVG and the size of its frame.
type Parts = (Image, Option<Image>, Option<(u32, u32)>);

/// An SVG `width` by `height`, holding a small rectangle.
fn svg(width: u32, height: u32) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"><rect width="10" height="10"/></svg>"#
    )
}

/// The four bytes of `value`, least significant first.
fn le(value: u32) -> [u8; 4] {
    [0, 8, 16, 24].map(|shift| u8::try_from((value >> shift) & 0xFF).unwrap())
}

/// The header of a lossless WebP `width` by `height` with an alpha channel, and no pixels.
fn webp_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&le(18));
    bytes.extend_from_slice(b"WEBPVP8L");
    bytes.extend_from_slice(&le(5));
    bytes.push(0x2F);
    bytes.extend_from_slice(&le((width - 1) | ((height - 1) << 14) | (1 << 28)));
    bytes
}

/// The size in pixels `drawn_size` gives an SVG `width` by `height`.
fn drawn(width: f32, height: f32) -> (u32, u32) {
    let size = drawn_size(Size::from_wh(width, height).unwrap()).unwrap();
    (size.width(), size.height())
}

/// The PNG `bytes` as its width, its height, its color type and its samples.
fn decoded(bytes: &[u8]) -> (u32, u32, ColorType, Vec<u8>) {
    let mut reader = Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut samples = alloc::vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut samples).unwrap();
    samples.truncate(info.buffer_size());
    (info.width, info.height, info.color_type, samples)
}

/// A memory filesystem holding each of `entries`, a name and its bytes, in `/n`.
fn files(entries: &[(&str, &[u8])]) -> MemorySystem {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/n")).unwrap();
    for &(name, bytes) in entries {
        system.write(&Path::new("/n").join(name), bytes).unwrap();
    }
    system
}

/// An opaque grey PNG `width` by `height`.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width, height);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let samples = alloc::vec![128_u8; usize::try_from(width * height * 3).unwrap()];
    writer.write_image_data(&samples).unwrap();
    writer.finish().unwrap();
    bytes
}

/// The image a stored picture shows, its SVG and the size of its frame; `None` for a linked
/// picture.
fn parts(picture: Picture) -> Option<Parts> {
    match picture {
        Picture::Linked(_) => None,
        Picture::Stored { shown, svg, size } => Some((shown, svg, size)),
    }
}

/// The image a stored picture shows and its SVG; `None` for a linked picture.
fn stored(picture: Picture) -> Option<(Image, Option<Image>)> {
    parts(picture).map(|(shown, svg, _)| (shown, svg))
}

/// The picture `target` names, read relative to `/n` of `system`, as [`stored`] gives it.
fn stored_at(system: &MemorySystem, target: &str) -> (Image, Option<Image>) {
    stored(read(system, Path::new("/n"), target).unwrap()).unwrap()
}

#[test]
fn a_web_address_is_linked_as_written() {
    let system = MemorySystem::new();
    for address in [
        "http://example.com/a.png",
        "https://example.com/a.png?size=2",
    ] {
        assert!(matches!(
            read(&system, Path::new("/n"), address).unwrap(),
            Picture::Linked(linked) if linked == address
        ));
    }
}

#[test]
fn a_png_jpeg_gif_tiff_or_bmp_file_is_stored_as_it_is() {
    let logo = png(3, 2);
    let formats = [
        ("a.png", logo.as_slice(), ImageFormat::Png),
        ("a.jpg", JPEG, ImageFormat::Jpeg),
        ("a.gif", GIF, ImageFormat::Gif),
        ("a.tif", TIFF, ImageFormat::Tiff),
        ("a.bmp", BMP, ImageFormat::Bmp),
    ];
    let system = files(&formats.each_ref().map(|&(name, bytes, _)| (name, bytes)));
    for (name, bytes, format) in formats {
        let (shown, svg) = stored_at(&system, name);
        assert_eq!((shown.format(), shown.bytes()), (&format, bytes));
        assert!(svg.is_none());
    }
}

#[test]
fn an_svg_is_stored_beside_a_png_of_its_size_drawn_from_it() {
    let system = files(&[("diagram.svg", SVG.as_bytes())]);
    let (shown, svg) = stored_at(&system, "diagram.svg");
    let kept = svg.unwrap();
    assert_eq!(
        (kept.format(), kept.content_type(), kept.bytes()),
        (&ImageFormat::Svg, "image/svg+xml", SVG.as_bytes())
    );
    assert_eq!(shown.format(), &ImageFormat::Png);
    let (width, height, color, samples) = decoded(shown.bytes());
    assert_eq!((width, height, color), (120, 80, ColorType::Rgba));
    assert!(
        samples
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel == &[0x2B, 0x6C, 0xB0, 0xFF])
    );
}

#[test]
fn a_webp_is_stored_as_one_png_of_its_pixels() {
    let pixels: Vec<u8> = (0..12_u8)
        .flat_map(|index| [index * 20, 255 - index * 20, index * 7, 128 + index])
        .collect();
    let mut webp = Vec::new();
    WebPEncoder::new(&mut webp)
        .encode(&pixels, 4, 3, WebPColor::Rgba8)
        .unwrap();
    let system = files(&[("team.webp", &webp)]);
    let (shown, svg) = stored_at(&system, "team.webp");
    assert!(svg.is_none());
    assert_eq!(shown.format(), &ImageFormat::Png);
    assert_eq!(decoded(shown.bytes()), (4, 3, ColorType::Rgba, pixels));
}

#[test]
fn a_data_address_gives_the_parts_of_its_decoded_bytes() {
    let system = MemorySystem::new();
    let logo = png(3, 2);
    let encoded = format!("data:image/png;base64,{}", STANDARD.encode(&logo));
    let (shown, svg) = stored(read(&system, Path::new("/n"), &encoded).unwrap()).unwrap();
    assert_eq!(
        (shown.format(), shown.bytes()),
        (&ImageFormat::Png, logo.as_slice())
    );
    assert!(svg.is_none());
    let (drawn, kept) = stored(read(&system, Path::new("/n"), SVG_ADDRESS).unwrap()).unwrap();
    assert_eq!(kept.unwrap().bytes(), SVG.as_bytes());
    let size = drawn.pixel_size().unwrap();
    assert_eq!((size.width, size.height), (120, 80));
}

#[test]
fn a_percent_without_two_hexadecimal_digits_after_it_stays_as_it_is() {
    assert_eq!(super::percent_decoded("%41%4%zz%%42"), b"A%4%zz%B".to_vec());
}

#[test]
fn a_missing_file_and_an_avif_file_each_fail_naming_the_target() {
    let system = files(&[("a.avif", AVIF)]);
    for target in ["missing.png", "a.avif"] {
        assert!(matches!(
            read(&system, Path::new("/n"), target),
            Err(MarkdownError::Image { target: named, error }) if named == target && !error.is_empty()
        ));
    }
}

#[test]
fn a_percent_encoded_target_reads_the_decoded_name() {
    let system = files(&[("my dot.png", &png(1, 1))]);
    let (shown, _) = stored_at(&system, "my%20dot.png");
    assert_eq!(shown.bytes(), png(1, 1).as_slice());
}

#[test]
fn a_file_whose_name_holds_a_percent_is_still_found() {
    let system = files(&[("100%.png", &png(1, 1))]);
    let (shown, _) = stored_at(&system, "100%.png");
    assert_eq!(shown.bytes(), png(1, 1).as_slice());
}

#[test]
fn a_target_missing_both_ways_fails_naming_it_as_written() {
    let system = MemorySystem::new();
    assert!(matches!(
        read(&system, Path::new("/n"), "gone%20.png"),
        Err(MarkdownError::Image { target, error }) if target == "gone%20.png" && !error.is_empty()
    ));
}

#[test]
fn a_huge_svg_is_drawn_at_4096_pixels_on_its_longer_side() {
    assert_eq!(drawn(100_000.0, 100_000.0), (4096, 4096));
}

#[test]
fn a_wide_svg_keeps_its_proportions_when_drawn_smaller() {
    assert_eq!(drawn(8192.0, 2048.0), (4096, 1024));
}

#[test]
fn a_small_svg_is_drawn_at_its_own_size() {
    assert_eq!(drawn(120.0, 80.0), (120, 80));
}

#[test]
fn a_huge_svg_is_stored_with_a_capped_png_and_its_own_size_as_the_frame() {
    let system = files(&[("big.svg", svg(100_000, 100_000).as_bytes())]);
    let (shown, svg, size) = parts(read(&system, Path::new("/n"), "big.svg").unwrap()).unwrap();
    assert!(svg.is_some());
    let png = shown.pixel_size().unwrap();
    assert_eq!((png.width, png.height), (4096, 4096));
    assert_eq!(size, Some((100_000, 100_000)));
}

#[test]
fn a_webp_whose_pixels_exceed_256_mib_fails_naming_the_target() {
    let system = files(&[("big.webp", &webp_header(16_383, 16_383))]);
    assert!(matches!(
        read(&system, Path::new("/n"), "big.webp"),
        Err(MarkdownError::Image { target, error }) if target == "big.webp" && error.contains("256 MiB")
    ));
}
