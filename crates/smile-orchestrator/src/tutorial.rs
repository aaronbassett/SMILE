//! Tutorial loading and management for the SMILE Loop orchestrator.
//!
//! This module provides types and functions for loading markdown tutorials,
//! extracting image references, and validating content constraints.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, SmileError};

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

impl Tutorial {
    /// Loads a tutorial from the given file path.
    ///
    /// Validates that:
    /// - The file exists
    /// - The file size is within the 100KB limit
    /// - The content is valid UTF-8
    ///
    /// Note: Image extraction is performed separately via [`Tutorial::extract_images`].
    ///
    /// # Errors
    ///
    /// Returns `SmileError::TutorialNotFound` if the file doesn't exist.
    /// Returns `SmileError::TutorialTooLarge` if the file exceeds 100KB.
    /// Returns `SmileError::TutorialEncodingError` if the file is not valid UTF-8.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::load_from_file(path)
    }

    /// Loads a tutorial from a specific file path.
    ///
    /// Internal implementation that handles all validation.
    fn load_from_file(path: &Path) -> Result<Self> {
        // Check if file exists
        let metadata = std::fs::metadata(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SmileError::tutorial_not_found(path)
            } else {
                SmileError::Io(e)
            }
        })?;

        // Check file size
        let file_size = metadata.len();
        if file_size > MAX_TUTORIAL_SIZE {
            return Err(SmileError::tutorial_too_large(
                path,
                file_size / 1024, // Convert to KB for error message
            ));
        }

        // Read file content as UTF-8
        let content = std::fs::read_to_string(path).map_err(|e| {
            // Check if it's an encoding error
            if e.kind() == std::io::ErrorKind::InvalidData {
                SmileError::tutorial_encoding(path)
            } else {
                SmileError::Io(e)
            }
        })?;

        // Canonicalize the path for consistent representation
        let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        // Safe to convert: file_size is validated to be <= 100KB, which fits in usize on all platforms
        #[allow(clippy::cast_possible_truncation)]
        let size_bytes = file_size as usize;

        Ok(Self {
            path: canonical_path,
            content,
            images: Vec::new(), // Images extracted separately
            size_bytes,
        })
    }

    /// Returns the directory containing the tutorial file.
    ///
    /// This is used as the base path for resolving relative image references.
    #[must_use]
    pub fn base_dir(&self) -> Option<&Path> {
        self.path.parent()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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

    #[test]
    fn test_load_valid_tutorial() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let tutorial_path = temp_dir.join("test_tutorial_valid.md");

        // Create a valid tutorial file
        let content = "# My Tutorial\n\nThis is a test tutorial.";
        let mut file = std::fs::File::create(&tutorial_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Load and verify
        let tutorial = Tutorial::load(&tutorial_path).unwrap();
        assert!(tutorial.path.ends_with("test_tutorial_valid.md"));
        assert_eq!(tutorial.content, content);
        assert_eq!(tutorial.size_bytes, content.len());
        assert!(tutorial.images.is_empty());

        // Cleanup
        std::fs::remove_file(&tutorial_path).ok();
    }

    #[test]
    fn test_load_nonexistent_tutorial() {
        let result = Tutorial::load("/nonexistent/path/tutorial.md");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::TutorialNotFound { path } if path.to_string_lossy().contains("tutorial.md")),
            "Expected TutorialNotFound, got: {err:?}"
        );
    }

    #[test]
    fn test_load_tutorial_too_large() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let tutorial_path = temp_dir.join("test_tutorial_large.md");

        // Create a file larger than 100KB
        let content = "x".repeat(150 * 1024); // 150KB
        let mut file = std::fs::File::create(&tutorial_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Load should fail with TutorialTooLarge
        let result = Tutorial::load(&tutorial_path);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::TutorialTooLarge { path, size_kb }
                if path.to_string_lossy().contains("test_tutorial_large.md") && *size_kb >= 146),
            "Expected TutorialTooLarge with size >= 146KB, got: {err:?}"
        );

        // Cleanup
        std::fs::remove_file(&tutorial_path).ok();
    }

    #[test]
    fn test_load_tutorial_at_size_limit() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let tutorial_path = temp_dir.join("test_tutorial_at_limit.md");

        // Create a file exactly at 100KB (should succeed)
        let content = "x".repeat(100 * 1024);
        let mut file = std::fs::File::create(&tutorial_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Load should succeed
        let result = Tutorial::load(&tutorial_path);
        assert!(
            result.is_ok(),
            "Tutorial at exactly 100KB should load successfully"
        );

        let tutorial = result.unwrap();
        assert_eq!(tutorial.size_bytes, 100 * 1024);

        // Cleanup
        std::fs::remove_file(&tutorial_path).ok();
    }

    #[test]
    fn test_load_tutorial_just_over_limit() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let tutorial_path = temp_dir.join("test_tutorial_over_limit.md");

        // Create a file just over 100KB (100KB + 1 byte)
        let content = "x".repeat(100 * 1024 + 1);
        let mut file = std::fs::File::create(&tutorial_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Load should fail
        let result = Tutorial::load(&tutorial_path);
        assert!(result.is_err(), "Tutorial over 100KB should fail to load");

        // Cleanup
        std::fs::remove_file(&tutorial_path).ok();
    }

    #[test]
    fn test_load_tutorial_invalid_encoding() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let tutorial_path = temp_dir.join("test_tutorial_invalid_utf8.md");

        // Create a file with invalid UTF-8 bytes
        let invalid_bytes: Vec<u8> = vec![0x80, 0x81, 0x82, 0xFF, 0xFE];
        let mut file = std::fs::File::create(&tutorial_path).unwrap();
        file.write_all(&invalid_bytes).unwrap();

        // Load should fail with encoding error
        let result = Tutorial::load(&tutorial_path);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::TutorialEncodingError { path }
                if path.to_string_lossy().contains("test_tutorial_invalid_utf8.md")),
            "Expected TutorialEncodingError, got: {err:?}"
        );

        // Cleanup
        std::fs::remove_file(&tutorial_path).ok();
    }

    #[test]
    fn test_base_dir() {
        let tutorial = Tutorial {
            path: PathBuf::from("/tutorials/getting-started/intro.md"),
            content: String::new(),
            images: vec![],
            size_bytes: 0,
        };

        let base_dir = tutorial.base_dir();
        assert!(base_dir.is_some());
        assert_eq!(base_dir.unwrap(), Path::new("/tutorials/getting-started"));
    }

    #[test]
    fn test_base_dir_root_file() {
        let tutorial = Tutorial {
            path: PathBuf::from("/tutorial.md"),
            content: String::new(),
            images: vec![],
            size_bytes: 0,
        };

        let base_dir = tutorial.base_dir();
        assert!(base_dir.is_some());
        assert_eq!(base_dir.unwrap(), Path::new("/"));
    }
}
