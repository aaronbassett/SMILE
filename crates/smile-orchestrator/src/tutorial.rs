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

    /// Extracts and loads all image references from the tutorial markdown.
    ///
    /// Parses the markdown content for image syntax (`![alt](path)`) and
    /// resolves each path relative to the tutorial file's directory.
    ///
    /// Only images with supported formats (PNG, JPG, GIF, SVG) are loaded.
    /// Missing images or unsupported formats are silently skipped.
    ///
    /// This method mutates the tutorial in place, populating the `images` field.
    pub fn extract_images(&mut self) {
        let base_dir = match self.base_dir() {
            Some(dir) => dir.to_path_buf(),
            None => return,
        };

        let references = extract_image_references(&self.content);
        let mut images = Vec::new();

        for reference in references {
            // Skip URLs (http://, https://, data:, etc.)
            if reference.starts_with("http://")
                || reference.starts_with("https://")
                || reference.starts_with("data:")
            {
                continue;
            }

            // Resolve the path relative to the tutorial directory
            let resolved_path = base_dir.join(&reference);

            // Check if it's a supported format
            let Some(format) = ImageFormat::from_path(&resolved_path) else {
                continue; // Skip unsupported formats
            };

            // Try to load the image data
            let Ok(data) = std::fs::read(&resolved_path) else {
                continue; // Skip missing images
            };

            // Canonicalize the path for consistent representation
            let resolved_path = std::fs::canonicalize(&resolved_path).unwrap_or(resolved_path);

            images.push(TutorialImage {
                reference,
                resolved_path,
                format,
                data,
            });
        }

        self.images = images;
    }

    /// Loads a tutorial and extracts all images in one operation.
    ///
    /// This is a convenience method that combines [`Tutorial::load`] and
    /// [`Tutorial::extract_images`].
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Tutorial::load`].
    pub fn load_with_images(path: impl AsRef<Path>) -> Result<Self> {
        let mut tutorial = Self::load(path)?;
        tutorial.extract_images();
        Ok(tutorial)
    }
}

/// Extracts all image reference paths from markdown content.
///
/// Parses markdown image syntax: `![alt text](path)` and `![](path)`
/// Returns a vector of the path strings (not including alt text).
fn extract_image_references(content: &str) -> Vec<String> {
    use regex::Regex;

    // Regex to match markdown image syntax: ![alt](path)
    // Captures the path portion in group 1
    // Pattern explanation:
    // - `!\[` - literal "!["
    // - `[^\]]*` - any chars except "]" (alt text)
    // - `\]\(` - literal "]("
    // - `([^)\s]+)` - capture group: path (no parens or whitespace)
    // - `(?:\s+[^)]*)?` - optional title after whitespace
    // - `\)` - closing paren
    let Ok(re) = Regex::new(r"!\[[^\]]*\]\(([^)\s]+)(?:\s+[^)]*)?\)") else {
        return Vec::new();
    };

    re.captures_iter(content)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
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

    #[test]
    fn test_extract_image_references_basic() {
        let content = r"
# Tutorial

Here's an image:
![Screenshot](./images/screenshot.png)

And another:
![](diagram.jpg)
";
        let refs = extract_image_references(content);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], "./images/screenshot.png");
        assert_eq!(refs[1], "diagram.jpg");
    }

    #[test]
    fn test_extract_image_references_with_title() {
        let content = r#"![Alt text](image.png "Title")"#;
        let refs = extract_image_references(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "image.png");
    }

    #[test]
    fn test_extract_image_references_urls() {
        let content = r"
![Remote](https://example.com/image.png)
![Local](./local.png)
![Data](data:image/png;base64,abc)
";
        let refs = extract_image_references(content);
        assert_eq!(refs.len(), 3);
        // URLs are extracted but filtered out during loading
        assert!(refs.contains(&"https://example.com/image.png".to_string()));
        assert!(refs.contains(&"./local.png".to_string()));
        assert!(refs.contains(&"data:image/png;base64,abc".to_string()));
    }

    #[test]
    fn test_extract_image_references_no_images() {
        let content = "# Tutorial\n\nJust text, no images.";
        let refs = extract_image_references(content);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_extract_image_references_multiple_per_line() {
        let content = r"![A](a.png) Some text ![B](b.jpg)";
        let refs = extract_image_references(content);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], "a.png");
        assert_eq!(refs[1], "b.jpg");
    }

    #[test]
    fn test_extract_images_with_files() {
        use std::io::Write;

        // Create a temp directory structure
        let temp_dir = std::env::temp_dir().join("smile_test_images");
        let images_dir = temp_dir.join("images");
        std::fs::create_dir_all(&images_dir).unwrap();

        // Create a tutorial file
        let tutorial_path = temp_dir.join("tutorial.md");
        let content = r"
# My Tutorial

![Screenshot](images/screen.png)
![Diagram](images/diagram.svg)
![Missing](images/missing.gif)
![Remote](https://example.com/remote.png)
";
        let mut file = std::fs::File::create(&tutorial_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Create image files (with minimal valid content)
        let png_path = images_dir.join("screen.png");
        std::fs::write(&png_path, [0x89, 0x50, 0x4E, 0x47]).unwrap(); // PNG magic

        let svg_path = images_dir.join("diagram.svg");
        std::fs::write(&svg_path, b"<svg></svg>").unwrap();

        // Load tutorial with images
        let tutorial = Tutorial::load_with_images(&tutorial_path).unwrap();

        // Should have 2 images (missing.gif and remote URL are skipped)
        assert_eq!(tutorial.images.len(), 2);

        // Check PNG image
        let png_img = tutorial
            .images
            .iter()
            .find(|i| i.reference == "images/screen.png");
        assert!(png_img.is_some());
        let png_img = png_img.unwrap();
        assert_eq!(png_img.format, ImageFormat::Png);
        assert_eq!(png_img.data, vec![0x89, 0x50, 0x4E, 0x47]);

        // Check SVG image
        let svg_img = tutorial
            .images
            .iter()
            .find(|i| i.reference == "images/diagram.svg");
        assert!(svg_img.is_some());
        let svg_img = svg_img.unwrap();
        assert_eq!(svg_img.format, ImageFormat::Svg);
        assert_eq!(svg_img.data, b"<svg></svg>");

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_extract_images_unsupported_format() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir().join("smile_test_unsupported");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let tutorial_path = temp_dir.join("tutorial.md");
        let content = "![Image](image.bmp)";
        let mut file = std::fs::File::create(&tutorial_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        // Create a BMP file (unsupported format)
        let bmp_path = temp_dir.join("image.bmp");
        std::fs::write(&bmp_path, b"BM").unwrap();

        let tutorial = Tutorial::load_with_images(&tutorial_path).unwrap();

        // BMP is not supported, so no images should be loaded
        assert!(tutorial.images.is_empty());

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
