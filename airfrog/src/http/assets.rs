// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Static assets
//!
//! Large static content is stored in `../../assets` and minified by `build.rs`
//! for inclusion in the binary.

use crate::http::{ContentType, Header, StaticFile};

/// Airfrog favicon
pub(crate) const FAVICON_CONTENT: &[u8] = include_bytes!("../../assets/favicon-32x32.png");
pub(crate) const FAVICON_PATH: &str =
    concat!("/static/favicon.", env!("CARGO_PKG_VERSION"), "-32x32.png");

/// Airfrog logo
pub(crate) const LOGO_CONTENT: &[u8] = include_bytes!("../../assets/airfrog_dark_bg_300.png");
pub(crate) const LOGO_PATH: &str = concat!("/static/logo.", env!("CARGO_PKG_VERSION"), "-d.png");

// Minified CSS and JS files follow.  While they live in `assets/`, they are
// minified by `build.rs` and output to `OUT_DIR`, then included from there
// below.

/// Airfrog CSS
pub(crate) const CSS_CONTENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/style.css"));
pub(crate) const CSS_PATH: &str = concat!("/static/style.", env!("CARGO_PKG_VERSION"), "-af.css");

/// Browser HTML
pub(crate) const BROWSER_HTML_CONTENT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/browser.html"));
pub(crate) const BROWSER_HTML_PATH: &str =
    concat!("/static/browser.", env!("CARGO_PKG_VERSION"), "-t.html");

/// Config Update JS
pub(crate) const CONFIG_UPDATE_JS_CONTENT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/config_update.js"));
pub(crate) const CONFIG_UPDATE_JS_PATH: &str =
    concat!("/static/config_update.", env!("CARGO_PKG_VERSION"), "-i.js");

/// Memory JS
pub(crate) const MEMORY_JS_CONTENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/memory.js"));
pub(crate) const MEMORY_JS_PATH: &str =
    concat!("/static/memory.", env!("CARGO_PKG_VERSION"), "-u.js");

/// Memory CSS
pub(crate) const MEMORY_CSS_CONTENT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/memory.css"));
pub(crate) const MEMORY_CSS_PATH: &str =
    concat!("/static/memory.", env!("CARGO_PKG_VERSION"), "-v.css");

/// Footer (and buttons) HTML
pub(crate) const FOOTER_HTML_CONTENT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/footer.html"));
pub(crate) const FOOTER_HTML_PATH: &str =
    concat!("/static/footer.", env!("CARGO_PKG_VERSION"), "-c.html");

/// RTT HTML
pub(crate) const RTT_HTML_CONTENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rtt.html"));
pub(crate) const RTT_HTML_PATH: &str =
    concat!("/static/rtt.", env!("CARGO_PKG_VERSION"), "-h.html");

/// RTT CSS
pub(crate) const RTT_JS_CONTENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rtt.js"));
pub(crate) const RTT_JS_PATH: &str = concat!("/static/rtt.", env!("CARGO_PKG_VERSION"), "-i.js");

// Standard static file cache header
const CACHE_YEAR: Header = Header {
    name: "Cache-Control",
    value: "public, max-age=31536000",
};

pub(crate) const LOGO: StaticFile = StaticFile {
    path: LOGO_PATH,
    content_type: ContentType::Png,
    content: LOGO_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const FAVICON: StaticFile = StaticFile {
    path: FAVICON_PATH,
    content_type: ContentType::Png,
    content: FAVICON_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const CSS: StaticFile = StaticFile {
    path: CSS_PATH,
    content_type: ContentType::Css,
    content: CSS_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const BROWSER_HTML: StaticFile = StaticFile {
    path: BROWSER_HTML_PATH,
    content_type: ContentType::Html,
    content: BROWSER_HTML_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const CONFIG_UPDATE_JS: StaticFile = StaticFile {
    path: CONFIG_UPDATE_JS_PATH,
    content_type: ContentType::JavaScript,
    content: CONFIG_UPDATE_JS_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const MEMORY_JS: StaticFile = StaticFile {
    path: MEMORY_JS_PATH,
    content_type: ContentType::JavaScript,
    content: MEMORY_JS_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const MEMORY_CSS: StaticFile = StaticFile {
    path: MEMORY_CSS_PATH,
    content_type: ContentType::Css,
    content: MEMORY_CSS_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const FOOTER_HTML: StaticFile = StaticFile {
    path: FOOTER_HTML_PATH,
    content_type: ContentType::Html,
    content: FOOTER_HTML_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const RTT_HTML: StaticFile = StaticFile {
    path: RTT_HTML_PATH,
    content_type: ContentType::Html,
    content: RTT_HTML_CONTENT,
    headers: &[CACHE_YEAR],
};
pub(crate) const RTT_JS: StaticFile = StaticFile {
    path: RTT_JS_PATH,
    content_type: ContentType::Css,
    content: RTT_JS_CONTENT,
    headers: &[CACHE_YEAR],
};

/// All of the static files used by airfrog
pub(crate) const STATIC_FILES: [StaticFile; 10] = [
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
