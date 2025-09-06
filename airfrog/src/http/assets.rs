// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Static assets
//!
//! Large static content is stored in `../../assets` and minified by `build.rs`
//! for inclusion in the binary.

use crate::http::{ContentType, Header, StaticFile};
use crate::{static_file, static_file_css_js_html_minified as static_minified};

// Images
static_file!(FAVICON, ContentType::Png, "", "favicon-32x32", "png", "a");
static_file!(
    LOGO,
    ContentType::Png,
    "",
    "airfrog_dark_bg_300",
    "png",
    "d"
);

// Minified CSS and JS files follow.  While they live in `assets/`, they are
// minified by `build.rs` and output to `OUT_DIR`, then included from there
// below.
static_minified!(CSS, ContentType::Css, "", "style", "css", "af");
static_minified!(BROWSER_HTML, ContentType::Html, "", "browser", "html", "t");
static_minified!(
    CONFIG_UPDATE_JS,
    ContentType::JavaScript,
    "",
    "config_update",
    "js",
    "i"
);
static_minified!(MEMORY_JS, ContentType::JavaScript, "", "memory", "js", "u");
static_minified!(MEMORY_CSS, ContentType::Css, "", "memory", "css", "v");
static_minified!(FOOTER_HTML, ContentType::Html, "", "footer", "html", "c");
static_minified!(RTT_HTML, ContentType::Html, "", "rtt", "html", "h");
static_minified!(RTT_JS, ContentType::JavaScript, "", "rtt", "js", "i");

/// All of the static files used by airfrog
pub(crate) const STATIC_FILES: &[StaticFile] = &[
    LOGO,
    FAVICON,
    CSS,
    BROWSER_HTML,
    CONFIG_UPDATE_JS,
    MEMORY_JS,
    MEMORY_CSS,
    FOOTER_HTML,
    RTT_HTML,
    RTT_JS,
];

// Standard static file cache header
pub const CACHE_YEAR: Header = Header {
    name: "Cache-Control",
    value: "public, max-age=31536000",
};

/// Generates static file constants for embedded web serving of asset files.
///
/// Creates three constants:
/// - `{NAME}_CONTENT`: `&[u8]` containing the file bytes via `include_bytes!`
/// - `{NAME}_PATH`: `&str` containing the versioned URL path
/// - `{NAME}`: `StaticFile` struct combining path, content, and headers
///
/// # Arguments
/// - `name`: Base name for generated constants
/// - `content_type`: MIME content type for HTTP headers
/// - `file_folder`: Path relative to `../../assets/` (use `""` for root assets folder)
/// - `file_name`: File name without extension (used for both file loading and URL)
/// - `content_suffix`: File extension (used for both file loading and URL)
/// - `cache_suffix`: Cache-busting suffix (increment when file changes)
///
/// # Example
/// ```
/// static_file!(
///     LOGO,
///     ContentType::Png,
///     "",
///     "logo",
///     "png",
///     "d"
/// );
/// ```
///
/// Generates URL: `/static/logo.{VERSION}-d.png`
#[macro_export]
macro_rules! static_file {
    ($name:ident, $content_type:expr, $file_folder:literal, $file_name:literal, $content_suffix:literal, $cache_suffix:literal) => {
        paste::paste! {
            pub(crate) const [<$name _CONTENT>]: &[u8] = include_bytes!(concat!("../../assets/", $file_folder, $file_name, ".", $content_suffix));
            pub(crate) const [<$name _PATH>]: &str = concat!(
                "/static/",
                $file_name,
                ".",
                env!("CARGO_PKG_VERSION"),
                "-",
                $cache_suffix,
                ".",
                $content_suffix
            );
            pub(crate) const $name: StaticFile = StaticFile {
                path: [<$name _PATH>],
                content_type: $content_type,
                content: [<$name _CONTENT>],
                headers: &[$crate::http::assets::CACHE_YEAR],
            };
        }
    };
}

/// Generates static file constants for embedded web serving of minified HTML
/// CSS and JS files.
///
/// Creates three constants:
/// - `{NAME}_CONTENT`: `&[u8]` containing the file bytes via `include_bytes!`
/// - `{NAME}_PATH`: `&str` containing the versioned URL path
/// - `{NAME}`: `StaticFile` struct combining path, content, and headers
///
/// # Arguments
/// - `name`: Base name for generated constants
/// - `content_type`: MIME content type for HTTP headers
/// - `file_folder`: Path to asset file
/// - `file_name`: File name without extension (used for both file loading and URL)
/// - `content_suffix`: File extension (used for both file loading and URL)
/// - `cache_suffix`: Cache-busting suffix (increment when file changes)
///
/// # Example
/// ```
/// static_file!(
///     ONEROM_LAB_READROM_JS,
///     ContentType::JavaScript,
///     "../../assets/firmware/",
///     "oneromlab_readrom",
///     "js",
///     "a"
/// );
/// ```
///
/// Generates URL: `/static/oneromlab_readrom.{VERSION}-a.js`
#[macro_export]
macro_rules! static_file_css_js_html_minified {
    ($name:ident, $content_type:expr, $file_folder:literal, $file_name:literal, $content_suffix:literal, $cache_suffix:literal) => {
        paste::paste! {
            pub(crate) const [<$name _CONTENT>]: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/", $file_folder, "/", $file_name, ".", $content_suffix));
            pub(crate) const [<$name _PATH>]: &str = concat!(
                "/static/",
                $file_name,
                ".",
                env!("CARGO_PKG_VERSION"),
                "-",
                $cache_suffix,
                ".",
                $content_suffix
            );
            pub(crate) const $name: StaticFile = StaticFile {
                path: [<$name _PATH>],
                content_type: $content_type,
                content: [<$name _CONTENT>],
                headers: &[$crate::http::assets::CACHE_YEAR],
            };
        }
    };
}
