// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - JSON routines routines

use alloc::format;
use alloc::string::String;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use serde_json::Value;

use crate::{AirfrogError, ErrorKind};

/// A function that provides basic JSON to HTML conversion.  Used by the
/// firmware default formatter, in case a firmware implementation does not
/// provide a custom HTML formatter.
pub(crate) fn json_to_html(value: Value, indent: usize) -> String {
    let indent_str = "  ".repeat(indent);

    match value {
        Value::Object(map) => {
            let mut html = format!("{indent_str}<div class=\"json-object\">\n");
            for (key, val) in map {
                html.push_str(&format!("{indent_str}  <div class=\"json-field\">\n"));
                html.push_str(&format!(
                    "{indent_str}    <span class=\"json-key\">{key}:</span>\n",
                ));
                html.push_str(&json_to_html(val, indent + 1));
                html.push_str(&format!("{indent_str}  </div>\n"));
            }
            html.push_str(&format!("{indent_str}</div>\n"));
            html
        }
        Value::Array(arr) => {
            let mut html = format!("{indent_str}<div class=\"json-array\">\n");
            for (ii, val) in arr.iter().enumerate() {
                html.push_str(&format!(
                    "{indent_str}  <div class=\"json-item\">[{ii}]:</div>\n",
                ));
                html.push_str(&json_to_html(val.clone(), indent + 1));
            }
            html.push_str(&format!("{indent_str}</div>\n"));
            html
        }
        Value::String(s) => {
            format!("{indent_str}<span class=\"json-string\">\"{s}\"</span>\n",)
        }
        Value::Number(n) => format!("{indent_str}<span class=\"json-number\">{n}</span>\n"),
        Value::Bool(b) => format!("{indent_str}<span class=\"json-bool\">{b}</span>\n"),
        Value::Null => format!("{indent_str}<span class=\"json-null\">null</span>\n"),
    }
}

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
