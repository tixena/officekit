//! A markdown picture's target read into the image parts Word stores for it.

#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use std::io::Cursor;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use image_webp::WebPDecoder;
use officekit_xml::packaging::image::{Image, ImageFormat};
use os_shim::System;
use png::{BitDepth, ColorType, Encoder};
use resvg::tiny_skia::{IntSize, Pixmap, Size, Transform};
use resvg::usvg::{Options, Tree};

use super::MarkdownError;

/// The longest side, in pixels, of the PNG drawn from an SVG.
const LONGEST_DRAWN: f32 = 4096.0;

/// Bytes 0 to 3 of a WebP file.
const RIFF: &[u8] = b"RIFF";

/// Why bytes are no picture.
const UNKNOWN_FORMAT: &str = "the bytes are none of PNG, JPEG, GIF, TIFF, BMP, WebP and SVG";

/// Bytes 8 to 11 of a WebP file.
const WEBP: &[u8] = b"WEBP";

/// The most bytes a WebP's decoded pixels may take.
const WEBP_MEMORY: usize = 256 * 1024 * 1024;

/// Why a relative path is no picture when the markdown has no folder.
const NO_FOLDER: &str =
    "a relative path, and markdown from standard input has no folder to find it from";

/// What each target that names a file holds: the file's bytes, or why they could not be read. The
/// conversion of a document cannot wait on a system, so its files are read before it starts, and a
/// failure stays here until the conversion reaches that picture.
pub type Files = BTreeMap<String, Result<Vec<u8>, String>>;

/// The parts one markdown picture needs.
pub enum Picture {
    /// A web address Word fetches when it opens the file.
    Linked(String),
    /// Image parts: `shown`, the part `a:blip r:embed` names, and for an SVG the SVG itself; `size`
    /// is the width and height in pixels of the picture's frame: an SVG's own size, otherwise
    /// `shown`'s.
    Stored {
        shown: Image,
        size: Option<(u32, u32)>,
        svg: Option<Image>,
    },
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
async fn file<S>(system: &S, path: &Path) -> Result<Vec<u8>, String>
where
    S: System + ?Sized,
{
    let (bytes, _revision) = system.read(path).await.map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// The files `targets` name, read through `system`, a relative path from `base`. A web address and
/// a `data:` address name no file.
pub async fn files<S>(system: &S, base: Option<&Path>, targets: BTreeSet<String>) -> Files
where
    S: System + ?Sized,
{
    let mut read = Files::new();
    for target in targets {
        if !is_web_address(&target) && !target.starts_with("data:") {
            let bytes = local(system, base, &target).await;
            read.insert(target, bytes);
        }
    }
    read
}

/// The value of an ASCII hexadecimal digit.
fn hex_digit(digit: char) -> Option<u8> {
    let value = digit.to_digit(16)?;
    u8::try_from(value).ok()
}

/// Whether `target` is a web address, which Word fetches when it opens the file.
fn is_web_address(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

/// The bytes of the file `target` names, read through `system`, a relative path from `base`: the
/// file of its percent-decoded name when that differs and exists, else the file of `target`.
async fn local<S>(system: &S, base: Option<&Path>, target: &str) -> Result<Vec<u8>, String>
where
    S: System + ?Sized,
{
    let folder = match base {
        Some(folder) => folder,
        None if Path::new(target).is_absolute() => Path::new(""),
        None => return Err(NO_FOLDER.to_owned()),
    };
    if let Ok(name) = String::from_utf8(percent_decoded(target))
        && name != target
    {
        let decoded = folder.join(name);
        if system.exists(&decoded).await.unwrap_or(false) {
            return file(system, &decoded).await;
        }
    }
    file(system, &folder.join(target)).await
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

/// The picture `target` names: a web address, a `data:` address, or a file [`files`] read; a file
/// is taken out of `files`, since a stored picture's target is not asked for twice.
///
/// # Errors
///
/// [`MarkdownError::Image`] naming `target` when it is a relative path and there was no folder to
/// find it from, its file could not be read, its `data:` address does not decode, or its bytes are
/// none of PNG, JPEG, GIF, TIFF, BMP, WebP and SVG.
pub fn read(files: &mut Files, target: &str) -> Result<Picture, MarkdownError> {
    if is_web_address(target) {
        return Ok(Picture::Linked(target.to_owned()));
    }
    let bytes = target.strip_prefix("data:").map_or_else(
        || {
            files
                .remove(target)
                .unwrap_or_else(|| Err("the file was not read".to_owned()))
        },
        data,
    );
    bytes
        .and_then(stored)
        .map_err(|error| MarkdownError::Image {
            target: target.to_owned(),
            error,
        })
}

/// The size, in pixels, of the PNG drawn from an SVG of `size`: `size` rounded, unless a side
/// passes [`LONGEST_DRAWN`], then scaled down to fit keeping its proportions.
fn drawn_size(size: Size) -> Option<IntSize> {
    if size.width().max(size.height()) > LONGEST_DRAWN {
        Some(
            size.scale_to(Size::from_wh(LONGEST_DRAWN, LONGEST_DRAWN)?)
                .to_int_size(),
        )
    } else {
        Some(size.to_int_size())
    }
}

/// A PNG drawn from `tree`, at most [`LONGEST_DRAWN`] pixels on its longer side.
fn rendered(tree: &Tree) -> Result<Image, String> {
    let drawn = drawn_size(tree.size()).ok_or_else(|| "the SVG has no size".to_owned())?;
    let mut pixmap = Pixmap::new(drawn.width(), drawn.height())
        .ok_or_else(|| "the SVG is too large to draw".to_owned())?;
    let own = Transform::from_bbox(tree.size().to_non_zero_rect(0.0, 0.0))
        .invert()
        .ok_or_else(|| "the SVG has no size".to_owned())?;
    let fit = Transform::from_bbox(drawn.to_size().to_non_zero_rect(0.0, 0.0));
    resvg::render(tree, fit.pre_concat(own), &mut pixmap.as_mut());
    let bytes = pixmap.encode_png().map_err(|error| error.to_string())?;
    Ok(Image::with_format(ImageFormat::Png, bytes))
}

/// The parts `bytes` give: a WebP as a PNG of its pixels; an SVG as itself beside a PNG drawn from
/// it; a PNG, JPEG, GIF, TIFF or BMP as it is.
fn stored(bytes: Vec<u8>) -> Result<Picture, String> {
    if bytes.get(0..4) == Some(RIFF) && bytes.get(8..12) == Some(WEBP) {
        let shown = webp_png(&bytes)?;
        return Ok(Picture::Stored {
            size: shown
                .pixel_size()
                .map(|pixels| (pixels.width, pixels.height)),
            shown,
            svg: None,
        });
    }
    if let Ok(tree) = Tree::from_data(&bytes, &svg_options()) {
        let own = tree.size().to_int_size();
        return Ok(Picture::Stored {
            shown: rendered(&tree)?,
            size: Some((own.width(), own.height())),
            svg: Some(Image::with_format(ImageFormat::Svg, bytes)),
        });
    }
    let Ok(shown) = Image::from_bytes(bytes) else {
        return Err(UNKNOWN_FORMAT.to_owned());
    };
    Ok(Picture::Stored {
        size: shown
            .pixel_size()
            .map(|pixels| (pixels.width, pixels.height)),
        shown,
        svg: None,
    })
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
    decoder.set_memory_limit(WEBP_MEMORY);
    let length = decoder
        .output_buffer_size()
        .filter(|length| *length <= WEBP_MEMORY)
        .ok_or_else(|| "the WebP's pixels exceed 256 MiB".to_owned())?;
    let mut pixels = alloc::vec![0; length];
    decoder
        .read_image(&mut pixels)
        .map_err(|error| error.to_string())?;
    png_of(&pixels, width, height, color)
}
