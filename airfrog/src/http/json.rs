// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - JSON routines routines

use alloc::string::String;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use serde_json::Value;

use crate::{AirfrogError, ErrorKind};

/// A function to parse the JSON body in an HTTP request.
pub(crate) fn parse_json_body(body: Option<String>) -> Result<Option<Value>, AirfrogError> {
    match body {
        Some(json_str) => match serde_json::from_str::<serde_json::Value>(&json_str) {
            Ok(json) => Ok(Some(json)),
            Err(e) => {
                debug!("Failed to parse JSON: {e:?}");
                Err(AirfrogError::Airfrog(ErrorKind::InvalidBody))
            }
        },
        None => Ok(None),
    }
}
