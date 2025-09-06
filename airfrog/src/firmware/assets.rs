//! Firmware static assets
//!
//! Firmware specific static content is stored in `../../assets/firmware` and
//! minified by `build.rs` for inclusion in the binary.
//!
//! To add static assets for custom firmware , add them below following the
//! existing patterns, and to the static content directory.  You need to:
//! - call `static_minified!` with appropriate parameters
//! - add the FILE constant to the FIRMWARE_STATIC_FILES array
//!
//! When modifying the static asset files, update the suffix in the path to
//! force browsers to reload the assets instead of using the cached version.
//!
//! `build.rs` automatically minifies any JS, CSS and HTML files placed in the
//! firmware assets directory, and the macro used below will include them.

// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

use crate::http::{ContentType, StaticFile};
use crate::static_file_css_js_html_minified as static_minified;

static_minified!(
    ONEROM_LAB_READROM_JS,
    ContentType::JavaScript,
    "firmware",
    "oneromlab_readrom",
    "js",
    "a"
);

pub(crate) const STATIC_FILES: &[StaticFile] = &[ONEROM_LAB_READROM_JS];
