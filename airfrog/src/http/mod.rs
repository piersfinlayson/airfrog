// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Top level web server objects and routines

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use embassy_time::Duration;
use embedded_io_async::Write;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use crate::AirfrogError;
use crate::target::Response as TargetResponse;
use html::HtmlContent;

pub(crate) mod assets;
pub(crate) mod html;
pub(crate) mod json;
pub(crate) mod server;

pub(crate) use server::start;

// Number of tasks in the web task pool.
pub(crate) const WEB_TASK_POOL_SIZE: usize = 4;

// Port for the HTTP server
pub(crate) const HTTPD_PORT: u16 = 80;

// Time to wait for the http server router to respond
// Temporarily make long so erase operations can complete
pub(crate) const ROUTER_TIMEOUT: Duration = Duration::from_millis(30000);

// Buffer sizes for the HTTP server tasks
const HTTPD_TASK_TCP_RX_BUF_SIZE: usize = 4096;
const HTTPD_TASK_TCP_TX_BUF_SIZE: usize = 4096;
const HTTPD_HEADER_BUF_SIZE: usize = 2048;
const HTTPD_BODY_BUF_SIZE: usize = 4096;

const HTTPD_MAX_HEADERS: usize = 32;

/// Supported HTTP methods for the REST API
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    Get,
    Post,
}

impl Method {
    pub fn from_str(method: &str) -> Option<Method> {
        match method {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            _ => None,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::Get => write!(f, "GET"),
            Method::Post => write!(f, "POST"),
        }
    }
}

/// Types of REST API requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rest {
    Target,
    Raw,
    SwdConfig,
}

#[derive(Debug, Clone, Copy)]
pub enum ContentType {
    Html,
    Json,
    _Text,
    JavaScript,
    Css,
    Png,
}

impl ContentType {
    pub const HTML: &'static str = "text/html";
    pub const JSON: &'static str = "application/json";
    pub const TEXT: &'static str = "text/plain";
    pub const JAVASCRIPT: &'static str = "application/javascript";
    pub const CSS: &'static str = "text/css";
    pub const PNG: &'static str = "image/png";

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Html => Self::HTML,
            Self::Json => Self::JSON,
            Self::_Text => Self::TEXT,
            Self::JavaScript => Self::JAVASCRIPT,
            Self::Css => Self::CSS,
            Self::Png => Self::PNG,
        }
    }
}

impl core::fmt::Display for ContentType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub enum ResponseContent {
    Owned(String),
    Borrowed(&'static [u8]),
}

#[derive(Debug, Clone)]
pub struct Response {
    pub path: Option<String>,
    pub status_code: StatusCode,
    pub content: Option<ResponseContent>,
    pub content_type: Option<ContentType>,
    pub headers: Option<Vec<Header>>,
}

impl Response {
    #[allow(unused)]
    pub fn ok(path: &str, body: String, content_type: ContentType) -> Self {
        Self {
            path: Some(path.to_string()),
            status_code: StatusCode::Ok,
            content: Some(ResponseContent::Owned(body)),
            content_type: Some(content_type),
            headers: None,
        }
    }

    pub fn not_found(response_format: Option<ContentType>, path: &str) -> Self {
        Self::status_code(response_format, Some(path), StatusCode::NotFound)
    }

    pub fn bad_request(response_format: Option<ContentType>, path: &str) -> Self {
        Self::status_code(response_format, Some(path), StatusCode::BadRequest)
    }

    pub fn too_large(response_format: Option<ContentType>, path: &str) -> Self {
        Self::status_code(response_format, Some(path), StatusCode::TooLarge)
    }

    pub fn redirect(path: &str, url: &'static str) -> Self {
        let header = Header {
            name: "Location",
            value: url,
        };
        Self {
            path: Some(path.to_string()),
            status_code: StatusCode::Found,
            content: None,
            content_type: None,
            headers: Some(vec![header]),
        }
    }

    pub fn status_code(
        response_format: Option<ContentType>,
        path: Option<&str>,
        status_code: StatusCode,
    ) -> Response {
        match response_format {
            Some(ContentType::Json) => {
                Self::json_already(path.unwrap_or_default(), "{}".to_string(), status_code)
            }
            Some(ContentType::Html) => match status_code {
                StatusCode::Timeout => Self::timeout_html(path),
                StatusCode::NotFound => Self::not_found_html(path),
                StatusCode::BadRequest => Self::bad_request_html(path),
                _ => Self::status_code_default(path, status_code),
            },
            _ => Self::status_code_default(path, status_code),
        }
    }

    pub fn status_code_default(path: Option<&str>, status_code: StatusCode) -> Response {
        Response {
            path: path.map(String::from),
            status_code,
            content: None,
            content_type: None,
            headers: None,
        }
    }

    fn timeout_html(path: Option<&str>) -> Response {
        Response::html(
            path.unwrap_or_default(),
            html::html_timeout(),
            StatusCode::Timeout,
        )
    }

    fn not_found_html(path: Option<&str>) -> Response {
        Response::html(
            path.unwrap_or_default(),
            html::html_not_found(),
            StatusCode::NotFound,
        )
    }

    fn bad_request_html(path: Option<&str>) -> Response {
        Response::html(
            path.unwrap_or_default(),
            html::html_bad_request(),
            StatusCode::BadRequest,
        )
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{path} {}", self.status_code)
        } else {
            write!(f, "No path {}", self.status_code)
        }
    }
}

impl Response {
    pub async fn write_to(
        &self,
        socket: &mut embassy_net::tcp::TcpSocket<'_>,
    ) -> Result<(), embassy_net::tcp::Error> {
        let content_len = match &self.content {
            Some(ResponseContent::Owned(s)) => s.len(),
            Some(ResponseContent::Borrowed(b)) => b.len(),
            None => 0,
        };

        let header_str = format!(
            "HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: {}\r\n",
            self.status_code.as_str(),
            content_len,
            self.content_type
                .as_ref()
                .map_or("text/plain", |ct| ct.as_str())
        );

        socket.write_all(header_str.as_bytes()).await?;

        // Add custom headers
        if let Some(ref headers) = self.headers {
            for header in headers {
                let header_line = format!("{}: {}\r\n", header.name, header.value);
                socket.write_all(header_line.as_bytes()).await?;
            }
        }
        socket.write_all(b"\r\n").await?;

        // Write content without copying
        match &self.content {
            Some(ResponseContent::Owned(s)) => socket.write_all(s.as_bytes()).await?,
            Some(ResponseContent::Borrowed(b)) => socket.write_all(b).await?,
            None => {}
        }

        Ok(())
    }
}

// Add helper for converting from various response types
impl Response {
    pub fn error(request_format: Option<ContentType>, path: &str, error: AirfrogError) -> Self {
        let response = Self::status_code(request_format, Some(path), error.clone().into());
        if matches!(request_format, Some(ContentType::Json)) {
            response.with_content(
                ContentType::Json,
                ResponseContent::Owned(serde_json::to_string(&error).unwrap_or_default()),
            )
        } else {
            response
        }
    }

    pub fn target_response(path: &str, response: TargetResponse) -> Self {
        let status_code = if let Some(ref error) = response.error {
            // Assuming AirfrogError::status_code() returns something we can map
            match error.status_code() {
                400 => StatusCode::BadRequest,
                404 => StatusCode::NotFound,
                500 => StatusCode::InternalServerError,
                _ => StatusCode::Ok,
            }
        } else {
            StatusCode::Ok
        };

        Self::json(path, &response, status_code)
    }

    pub fn json<T: serde::Serialize>(path: &str, data: T, status_code: StatusCode) -> Self {
        let content = serde_json::to_string(&data)
            .unwrap_or("{\"error\":\"Failed to serialize\"}".to_string());
        Self::json_already(path, content, status_code)
    }

    pub fn json_already(path: &str, content: String, status_code: StatusCode) -> Self {
        Self {
            path: Some(path.to_string()),
            status_code,
            content: Some(ResponseContent::Owned(content)),
            content_type: Some(ContentType::Json),
            headers: None,
        }
    }

    pub fn html(path: &str, content: HtmlContent, status_code: StatusCode) -> Self {
        Self {
            path: Some(path.to_string()),
            status_code,
            content: Some(ResponseContent::Owned(content.0)),
            content_type: Some(ContentType::Html),
            headers: None,
        }
    }

    pub fn html_ok(path: &str, content: HtmlContent) -> Self {
        Self::html(path, content, StatusCode::Ok)
    }

    pub fn no_cache(mut self) -> Self {
        let no_cache_header = Header {
            name: "Cache-Control",
            value: "no-cache, no-store, must-revalidate",
        };
        let pragma_no_cache = Header {
            name: "Pragma",
            value: "no-cache",
        };
        let expires_0 = Header {
            name: "Expires",
            value: "0",
        };

        let mut new_headers = vec![no_cache_header, pragma_no_cache, expires_0];

        match self.headers {
            Some(mut existing) => {
                existing.append(&mut new_headers);
                self.headers = Some(existing);
            }
            None => {
                self.headers = Some(new_headers);
            }
        }

        self
    }

    pub fn static_file(path: &str, file: StaticFile) -> Self {
        let headers = if !file.headers.is_empty() {
            Some(
                file.headers
                    .iter()
                    .map(|h| Header {
                        name: h.name,
                        value: h.value,
                    })
                    .collect(),
            )
        } else {
            None
        };

        Response {
            path: Some(path.to_string()),
            status_code: StatusCode::Ok,
            content: Some(ResponseContent::Borrowed(file.content)),
            content_type: Some(file.content_type),
            headers,
        }
    }

    fn with_content(mut self, content_type: ContentType, content: ResponseContent) -> Self {
        self.content_type = Some(content_type);
        self.content = Some(content);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StatusCode {
    Ok = 200,
    Found = 302,
    BadRequest = 400,
    NotFound = 404,
    _InvalidMethod = 405,
    Timeout = 408,
    TooLarge = 413,
    InternalServerError = 500,
    ServiceUnavailable = 503,
}

impl From<AirfrogError> for StatusCode {
    fn from(error: AirfrogError) -> Self {
        Self::from_u16(error.status_code())
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl StatusCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "200 OK",
            Self::Found => "302 Found",
            Self::BadRequest => "400 Bad Request",
            Self::NotFound => "404 Not Found",
            Self::_InvalidMethod => "405 Method Not Allowed",
            Self::Timeout => "408 Request Timeout",
            Self::TooLarge => "413 Payload Too Large",
            Self::ServiceUnavailable => "503 Service Unavailable",
            Self::InternalServerError => "500 Internal Server Error",
        }
    }
}

impl StatusCode {
    pub fn from_u16(code: u16) -> Self {
        match code {
            200 => Self::Ok,
            302 => Self::Found,
            400 => Self::BadRequest,
            404 => Self::NotFound,
            405 => Self::_InvalidMethod,
            408 => Self::Timeout,
            413 => Self::TooLarge,
            500 => Self::InternalServerError,
            503 => Self::ServiceUnavailable,
            _ => Self::InternalServerError,
        }
    }
}

#[derive(Debug)]
pub struct StaticFile {
    pub path: &'static str,
    pub content_type: ContentType,
    pub content: &'static [u8],
    pub headers: &'static [Header],
}

#[derive(Debug, Clone, Copy)]
pub struct Header {
    pub name: &'static str,
    pub value: &'static str,
}
