//! Shared GUI-only helpers for authoring managed reference images.

use crate::mkmacro::MkImageRef;

/// Normalizes a user-entered managed image filename while retaining the flat
/// library's safety rules in MkImageRef.
pub(crate) fn normalize_image_filename(input: &str) -> Result<MkImageRef, String> {
    let trimmed = input.trim();
    let filename = if trimmed.to_ascii_lowercase().ends_with(".png") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}.png")
    };
    MkImageRef::new(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_preserves_png_suffix_and_flat_library_validation() {
        assert_eq!(
            normalize_image_filename(" login_button.PNG ")
                .unwrap()
                .filename(),
            "login_button.PNG"
        );
        assert_eq!(
            normalize_image_filename("login_button").unwrap().filename(),
            "login_button.png"
        );
        assert!(normalize_image_filename("nested/file").is_err());
        assert!(normalize_image_filename("").is_err());
    }
}
