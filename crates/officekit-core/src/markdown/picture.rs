//! A markdown picture's target read into the image parts Word stores for it.

#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use std::io::{Cursor, Read as _};
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use image_webp::WebPDecoder;
use officekit_packaging::image::{Image, ImageFormat};
use os_shim::System;
use png::{BitDepth, ColorType, Encoder};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

use super::MarkdownError;

/// Bytes 0 to 3 of a WebP file.
const RIFF: &[u8] = b"RIFF";

/// Why bytes are no picture.
const UNKNOWN_FORMAT: &str = "the bytes are none of PNG, JPEG, GIF, TIFF, BMP, WebP and SVG";

/// Bytes 8 to 11 of a WebP file.
const WEBP: &[u8] = b"WEBP";

/// The parts one markdown picture needs.
pub(super) enum Picture {
    /// A web address Word fetches when it opens the file.
    Linked(String),
    /// Image parts: `shown`, the part `a:blip r:embed` names, and for an SVG the SVG itself.
    Stored { shown: Image, svg: Option<Image> },
}

/// The bytes of a `data:` address, `address` being what follows `data:`: the bytes after the
/// first `,`, base64-decoded when the part before it ends in `;base64`, percent-decoded otherwise.
fn data(address: &str) -> Result<Vec<u8>, String> {
    let Some((header, payload)) = address.split_once(',') else {
        return Err("the data: address holds no `,`".to_owned());
    };
    if header.ends_with(";base64") {
        STANDARD.decode(payload).map_err(|error| error.to_string())
    } else {
        Ok(percent_decoded(payload))
    }
}

/// The bytes of the file at `path`, read through `system`.
fn file<S>(system: &S, path: &Path) -> Result<Vec<u8>, String>
where
    S: System + ?Sized,
{
    let mut bytes = Vec::new();
    system
        .open(path)
        .and_then(|mut reader| reader.read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// The value of an ASCII hexadecimal digit.
fn hex_digit(digit: char) -> Option<u8> {
    let value = digit.to_digit(16)?;
    u8::try_from(value).ok()
}

/// `text` with each `%` and the two hexadecimal digits after it replaced by the byte they name; a
/// `%` without two such digits stays as it is.
fn percent_decoded(text: &str) -> Vec<u8> {
    let mut pieces = text.split('%');
    let mut decoded = pieces.next().unwrap_or_default().as_bytes().to_vec();
    for piece in pieces {
        let mut digits = piece.chars();
        let escaped = digits
            .next()
            .and_then(hex_digit)
            .zip(digits.next().and_then(hex_digit))
            .map(|(high, low)| high * 16 + low);
        if let Some(byte) = escaped {
            decoded.push(byte);
            decoded.extend_from_slice(piece.get(2..).unwrap_or_default().as_bytes());
        } else {
            decoded.push(b'%');
            decoded.extend_from_slice(piece.as_bytes());
        }
    }
    decoded
}

/// `pixels`, rows of 8-bit samples of `color`, `width` by `height`, as a PNG.
fn png_of(pixels: &[u8], width: u32, height: u32, color: ColorType) -> Result<Image, String> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width, height);
    encoder.set_color(color);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(pixels)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())?;
    Ok(Image::with_format(ImageFormat::Png, bytes))
}

/// The picture `target` names: a web address, a `data:` address, or a file read through `system`
/// relative to `base`.
///
/// # Errors
///
/// [`MarkdownError::Image`] naming `target` when its file cannot be read, its `data:` address
/// does not decode, or its bytes are none of PNG, JPEG, GIF, TIFF, BMP, WebP and SVG.
pub(super) fn read<S>(system: &S, base: &Path, target: &str) -> Result<Picture, MarkdownError>
where
    S: System + ?Sized,
{
    if target.starts_with("http://") || target.starts_with("https://") {
        return Ok(Picture::Linked(target.to_owned()));
    }
    let bytes = target
        .strip_prefix("data:")
        .map_or_else(|| file(system, &base.join(target)), data);
    bytes
        .and_then(stored)
        .map_err(|error| MarkdownError::Image {
            target: target.to_owned(),
            error,
        })
}

/// A PNG of the size of `tree`, drawn from it.
fn rendered(tree: &Tree) -> Result<Image, String> {
    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height())
        .ok_or_else(|| "the SVG is too large to draw".to_owned())?;
    resvg::render(tree, Transform::default(), &mut pixmap.as_mut());
    let bytes = pixmap.encode_png().map_err(|error| error.to_string())?;
    Ok(Image::with_format(ImageFormat::Png, bytes))
}

/// The parts `bytes` give: a WebP as a PNG of its pixels; an SVG as itself beside a PNG drawn from
/// it; a PNG, JPEG, GIF, TIFF or BMP as it is.
fn stored(bytes: Vec<u8>) -> Result<Picture, String> {
    if bytes.get(0..4) == Some(RIFF) && bytes.get(8..12) == Some(WEBP) {
        return Ok(Picture::Stored {
            shown: webp_png(&bytes)?,
            svg: None,
        });
    }
    if let Ok(tree) = Tree::from_data(&bytes, &svg_options()) {
        return Ok(Picture::Stored {
            shown: rendered(&tree)?,
            svg: Some(Image::with_format(ImageFormat::Svg, bytes)),
        });
    }
    let Ok(shown) = Image::from_bytes(bytes) else {
        return Err(UNKNOWN_FORMAT.to_owned());
    };
    Ok(Picture::Stored { shown, svg: None })
}

/// usvg's default options, but an `<image>` that names a file is left out of the PNG: usvg would
/// read the file itself, outside `os_shim::System`.
fn svg_options() -> Options<'static> {
    let mut options = Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options
}

/// The pixels of the WebP `bytes` as a PNG.
fn webp_png(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = WebPDecoder::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let (width, height) = decoder.dimensions();
    let color = if decoder.has_alpha() {
        ColorType::Rgba
    } else {
        ColorType::Rgb
    };
    let length = decoder
        .output_buffer_size()
        .ok_or_else(|| "the WebP is too large to decode".to_owned())?;
    let mut pixels = alloc::vec![0; length];
    decoder
        .read_image(&mut pixels)
        .map_err(|error| error.to_string())?;
    png_of(&pixels, width, height, color)
}
