use super::{Image, ImageError, ImageFormat, PixelSize};

const PNG_PREFIX: [u8; 16] = *b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const PNG_SUFFIX: [u8; 5] = [8, 6, 0, 0, 0];
const JPEG_PREFIX: [u8; 7] = [0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8];
const WIDE: [u8; 8] = [0, 0, 1, 0x2C, 0, 0, 0, 100];
const PORTRAIT: [u8; 8] = [0, 0, 6, 0x1B, 0, 0, 8, 2];
const SQUARE: [u8; 4] = [1, 0xC8, 1, 0xC8];
const JPEG_SUFFIX: [u8; 10] = [3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1];

/// A PNG whose header holds the width then the height, four bytes each.
fn png(width_and_height: [u8; 8]) -> Vec<u8> {
    [&PNG_PREFIX[..], &width_and_height, &PNG_SUFFIX].concat()
}

/// A JPEG whose start-of-frame segment holds `height_and_width`, two bytes each.
fn jpeg(height_and_width: [u8; 4]) -> Vec<u8> {
    [&JPEG_PREFIX[..], &height_and_width, &JPEG_SUFFIX].concat()
}

#[test]
fn content_types_map_to_formats() {
    let cases = [
        ("image/bmp", ImageFormat::Bmp),
        ("image/x-emf", ImageFormat::Emf),
        ("image/gif", ImageFormat::Gif),
        ("image/jpeg", ImageFormat::Jpeg),
        ("image/vnd.ms-photo", ImageFormat::JpegXr),
        ("image/png", ImageFormat::Png),
        ("image/svg+xml", ImageFormat::Svg),
        ("image/tiff", ImageFormat::Tiff),
        ("image/x-wmf", ImageFormat::Wmf),
        ("image/avif", ImageFormat::Other("image/avif".to_owned())),
    ];
    for (content_type, format) in cases {
        assert_eq!(ImageFormat::from_content_type(content_type), Some(format));
    }
    assert_eq!(ImageFormat::from_content_type("application/xml"), None);
    assert_eq!(ImageFormat::from_content_type("imagepng"), None);
}

#[test]
fn extensions_are_the_lower_case_name_wdp_for_jpeg_xr_and_the_subtype_for_other() {
    let cases = [
        (ImageFormat::Bmp, "bmp"),
        (ImageFormat::Emf, "emf"),
        (ImageFormat::Gif, "gif"),
        (ImageFormat::Jpeg, "jpeg"),
        (ImageFormat::JpegXr, "wdp"),
        (ImageFormat::Png, "png"),
        (ImageFormat::Svg, "svg"),
        (ImageFormat::Tiff, "tiff"),
        (ImageFormat::Wmf, "wmf"),
        (ImageFormat::Other("image/avif".to_owned()), "avif"),
    ];
    for (format, extension) in cases {
        assert_eq!(format.extension(), extension);
    }
}

#[test]
fn a_png_gives_its_format_and_its_header_size() {
    let image = Image::from_bytes(png(WIDE)).unwrap();
    assert_eq!(image.format(), &ImageFormat::Png);
    assert_eq!(image.content_type(), "image/png");
    assert_eq!(
        image.pixel_size(),
        Some(PixelSize {
            height: 100,
            width: 300
        })
    );
}

#[test]
fn a_portrait_png_and_a_square_jpeg_give_their_sizes() {
    assert_eq!(
        Image::from_bytes(png(PORTRAIT)).unwrap().pixel_size(),
        Some(PixelSize {
            height: 2050,
            width: 1563
        })
    );
    let image = Image::from_bytes(jpeg(SQUARE)).unwrap();
    assert_eq!(image.format(), &ImageFormat::Jpeg);
    assert_eq!(
        image.pixel_size(),
        Some(PixelSize {
            height: 456,
            width: 456
        })
    );
}

#[test]
fn bytes_with_no_known_header_are_refused() {
    assert_eq!(
        Image::from_bytes(Vec::new()).unwrap_err(),
        ImageError::UnknownHeader
    );
    assert_eq!(
        Image::from_bytes(b"<svg/>".to_vec()).unwrap_err(),
        ImageError::UnknownHeader
    );
}

#[test]
fn with_format_takes_any_format_and_the_formats_without_a_header_reader_have_no_size() {
    for format in [
        ImageFormat::Emf,
        ImageFormat::JpegXr,
        ImageFormat::Svg,
        ImageFormat::Wmf,
    ] {
        let image = Image::with_format(format.clone(), png(WIDE));
        assert_eq!(image.format(), &format);
        assert_eq!(image.pixel_size(), None);
    }
}

#[test]
fn a_header_that_cannot_be_read_has_no_size() {
    let image = Image::with_format(ImageFormat::Png, b"\x89PNG\r\n\x1a\n".to_vec());
    assert_eq!(image.pixel_size(), None);
}
