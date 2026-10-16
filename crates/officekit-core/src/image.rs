//! Image parts as images: a format, the bytes, and the pixel size read from the header.

#[cfg(test)]
mod tests;

/// An image part's format and bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    bytes: Vec<u8>,
    format: ImageFormat,
}

/// The format of an image part, from its content type; any other `image/` type is `Other`, holding
/// that content type.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageFormat {
    Bmp,
    Emf,
    Gif,
    Jpeg,
    JpegXr,
    Other(String),
    Png,
    Svg,
    Tiff,
    Wmf,
}

/// An image's size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PixelSize {
    pub height: u32,
    pub width: u32,
}

/// Why bytes are not an image.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageError {
    /// The header names none of PNG, JPEG, GIF, TIFF, and BMP.
    #[error("the header names none of PNG, JPEG, GIF, TIFF, and BMP")]
    UnknownHeader,
}

impl ImageFormat {
    /// The extension a part name of this format takes: the format's name in lower case, `wdp` for
    /// JPEG XR, the subtype for `Other`.
    #[must_use]
    pub fn extension(&self) -> &str {
        match self {
            Self::Bmp => "bmp",
            Self::Emf => "emf",
            Self::Gif => "gif",
            Self::Jpeg => "jpeg",
            Self::JpegXr => "wdp",
            Self::Other(content_type) => {
                content_type.strip_prefix("image/").unwrap_or(content_type)
            }
            Self::Png => "png",
            Self::Svg => "svg",
            Self::Tiff => "tiff",
            Self::Wmf => "wmf",
        }
    }

    /// The format a content type names; `None` when it does not start with `image/`.
    #[must_use]
    pub fn from_content_type(content_type: &str) -> Option<Self> {
        let subtype = content_type.strip_prefix("image/")?;
        Some(match subtype {
            "bmp" => Self::Bmp,
            "gif" => Self::Gif,
            "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "svg+xml" => Self::Svg,
            "tiff" => Self::Tiff,
            "vnd.ms-photo" => Self::JpegXr,
            "x-emf" => Self::Emf,
            "x-wmf" => Self::Wmf,
            _ => Self::Other(content_type.to_owned()),
        })
    }
}

impl Image {
    /// The image's bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The image's content type.
    #[must_use]
    pub fn content_type(&self) -> &str {
        match &self.format {
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Emf => "image/x-emf",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::JpegXr => "image/vnd.ms-photo",
            ImageFormat::Other(content_type) => content_type,
            ImageFormat::Png => "image/png",
            ImageFormat::Svg => "image/svg+xml",
            ImageFormat::Tiff => "image/tiff",
            ImageFormat::Wmf => "image/x-wmf",
        }
    }

    /// The image's format.
    #[must_use]
    pub const fn format(&self) -> &ImageFormat {
        &self.format
    }

    /// Bytes whose header names PNG, JPEG, GIF, TIFF, or BMP.
    ///
    /// # Errors
    ///
    /// [`ImageError::UnknownHeader`] for any other bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ImageError> {
        let Ok(image_type) = imagesize::image_type(&bytes) else {
            return Err(ImageError::UnknownHeader);
        };
        let format = match image_type {
            imagesize::ImageType::Bmp => ImageFormat::Bmp,
            imagesize::ImageType::Gif => ImageFormat::Gif,
            imagesize::ImageType::Jpeg => ImageFormat::Jpeg,
            imagesize::ImageType::Png => ImageFormat::Png,
            imagesize::ImageType::Tiff => ImageFormat::Tiff,
            _ => return Err(ImageError::UnknownHeader),
        };
        Ok(Self { bytes, format })
    }

    /// The size the header gives; `None` for EMF, WMF, JPEG XR, SVG, and a header that cannot be
    /// read.
    #[must_use]
    pub fn pixel_size(&self) -> Option<PixelSize> {
        if !matches!(
            self.format,
            ImageFormat::Bmp
                | ImageFormat::Gif
                | ImageFormat::Jpeg
                | ImageFormat::Png
                | ImageFormat::Tiff
        ) {
            return None;
        }
        let size = imagesize::blob_size(&self.bytes).ok()?;
        Some(PixelSize {
            height: u32::try_from(size.height).ok()?,
            width: u32::try_from(size.width).ok()?,
        })
    }

    /// Bytes in a format the caller states, for the formats a header does not name.
    #[must_use]
    pub const fn with_format(format: ImageFormat, bytes: Vec<u8>) -> Self {
        Self { bytes, format }
    }
}
