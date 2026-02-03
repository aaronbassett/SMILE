//! Tutorial loading and management for the SMILE Loop orchestrator.
//!
//! This module provides types and functions for loading markdown tutorials,
//! extracting image references, and validating content constraints.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Maximum allowed tutorial file size in bytes (100KB).
pub const MAX_TUTORIAL_SIZE: u64 = 100 * 1024;

/// In-memory representation of a loaded tutorial.
///
/// Contains the raw markdown content, resolved image references,
/// and metadata about the tutorial file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tutorial {
    /// Path to the tutorial file.
    pub path: PathBuf,

    /// Raw markdown content of the tutorial.
    pub content: String,

    /// Images referenced in the tutorial markdown.
    pub images: Vec<TutorialImage>,

    /// Size of the tutorial file in bytes.
    pub size_bytes: usize,
}

/// An image referenced within a tutorial.
///
/// Contains both the reference as it appears in the markdown and
/// the resolved absolute path along with the loaded image data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TutorialImage {
    /// Path as it appears in the markdown (e.g., "./images/fig1.png").
    pub reference: String,

    /// Resolved absolute path to the image file.
    pub resolved_path: PathBuf,

    /// Detected image format.
    pub format: ImageFormat,

    /// Raw image bytes (loaded on demand).
    #[serde(skip)]
    pub data: Vec<u8>,
}

/// Supported image formats for tutorial images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    /// PNG image format.
    Png,
    /// JPEG image format.
    Jpg,
    /// GIF image format.
    Gif,
    /// SVG image format.
    Svg,
}

impl ImageFormat {
    /// Attempts to detect image format from file extension.
    ///
    /// Returns `None` if the extension is not recognized.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpg),
            "gif" => Some(Self::Gif),
            "svg" => Some(Self::Svg),
            _ => None,
        }
    }

    /// Attempts to detect image format from a file path.
    ///
    /// Returns `None` if the path has no extension or the extension is not recognized.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png => write!(f, "png"),
            Self::Jpg => write!(f, "jpg"),
            Self::Gif => write!(f, "gif"),
            Self::Svg => write!(f, "svg"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_format_from_extension() {
        assert_eq!(ImageFormat::from_extension("png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension("PNG"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension("jpg"), Some(ImageFormat::Jpg));
        assert_eq!(ImageFormat::from_extension("jpeg"), Some(ImageFormat::Jpg));
        assert_eq!(ImageFormat::from_extension("JPEG"), Some(ImageFormat::Jpg));
        assert_eq!(ImageFormat::from_extension("gif"), Some(ImageFormat::Gif));
        assert_eq!(ImageFormat::from_extension("svg"), Some(ImageFormat::Svg));
        assert_eq!(ImageFormat::from_extension("bmp"), None);
        assert_eq!(ImageFormat::from_extension("webp"), None);
    }

    #[test]
    fn test_image_format_from_path() {
        assert_eq!(
            ImageFormat::from_path(Path::new("image.png")),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            ImageFormat::from_path(Path::new("/path/to/photo.jpg")),
            Some(ImageFormat::Jpg)
        );
        assert_eq!(
            ImageFormat::from_path(Path::new("./relative/anim.gif")),
            Some(ImageFormat::Gif)
        );
        assert_eq!(
            ImageFormat::from_path(Path::new("diagram.SVG")),
            Some(ImageFormat::Svg)
        );
        assert_eq!(ImageFormat::from_path(Path::new("no_extension")), None);
        assert_eq!(ImageFormat::from_path(Path::new("unsupported.bmp")), None);
    }

    #[test]
    fn test_image_format_display() {
        assert_eq!(ImageFormat::Png.to_string(), "png");
        assert_eq!(ImageFormat::Jpg.to_string(), "jpg");
        assert_eq!(ImageFormat::Gif.to_string(), "gif");
        assert_eq!(ImageFormat::Svg.to_string(), "svg");
    }

    #[test]
    fn test_tutorial_struct_default_state() {
        let tutorial = Tutorial {
            path: PathBuf::from("test.md"),
            content: "# Test Tutorial".to_string(),
            images: vec![],
            size_bytes: 15,
        };

        assert_eq!(tutorial.path, PathBuf::from("test.md"));
        assert_eq!(tutorial.content, "# Test Tutorial");
        assert!(tutorial.images.is_empty());
        assert_eq!(tutorial.size_bytes, 15);
    }

    #[test]
    fn test_tutorial_with_images() {
        let tutorial = Tutorial {
            path: PathBuf::from("tutorial.md"),
            content: "# Tutorial\n![Image](./img.png)".to_string(),
            images: vec![TutorialImage {
                reference: "./img.png".to_string(),
                resolved_path: PathBuf::from("/absolute/path/img.png"),
                format: ImageFormat::Png,
                data: vec![0x89, 0x50, 0x4E, 0x47], // PNG magic bytes
            }],
            size_bytes: 30,
        };

        assert_eq!(tutorial.images.len(), 1);
        assert_eq!(tutorial.images[0].reference, "./img.png");
        assert_eq!(tutorial.images[0].format, ImageFormat::Png);
        assert!(!tutorial.images[0].data.is_empty());
    }

    #[test]
    fn test_max_tutorial_size_constant() {
        assert_eq!(MAX_TUTORIAL_SIZE, 102_400);
    }
}
