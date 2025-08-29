// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Web server implementation

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use embassy_executor::Spawner;
use embassy_net::{Stack, tcp::TcpSocket};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_sync::signal::Signal;
use embassy_time::with_timeout;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
#[cfg(feature = "httpd")]
use static_assertions::const_assert_eq;
use static_cell::make_static;

use crate::config::{CONFIG, Net};
use crate::device::DEVICE;
use crate::http::assets::{FAVICON, STATIC_FILES};
use crate::http::html::{
    html_summary, page_dashboard, page_info, page_settings, page_target_browser,
    page_target_firmware, page_target_rtt, page_target_update,
};
use crate::http::json::parse_json_body;
use crate::http::{
    ContentType, HTTPD_BODY_BUF_SIZE, HTTPD_HEADER_BUF_SIZE, HTTPD_MAX_HEADERS, HTTPD_PORT,
    HTTPD_TASK_TCP_RX_BUF_SIZE, HTTPD_TASK_TCP_TX_BUF_SIZE, ROUTER_TIMEOUT, WEB_TASK_POOL_SIZE,
};
use crate::http::{Header, HtmlContent, Method, Response, ResponseContent, Rest, StatusCode};
use crate::rtt::{Command as RttCommand, Error as RttError, Response as RttResponse, rtt_command};
use crate::target::{
    Command, REQUEST_CHANNEL_SIZE, Request as TargetRequest, Response as TargetResponse,
};
use crate::{AirfrogError, ErrorKind, REBOOT_SIGNAL};

/// Main HTTP server object that handles incoming connections and requests.
struct Server {
    target_sender: Sender<'static, CriticalSectionRawMutex, TargetRequest, REQUEST_CHANNEL_SIZE>,
    response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
    rtt_rsp_ch: &'static Channel<CriticalSectionRawMutex, Result<RttResponse, RttError>, 1>,
    header_buf: &'static mut [u8; HTTPD_HEADER_BUF_SIZE],
    body_buf: &'static mut [u8; HTTPD_BODY_BUF_SIZE],
}

impl Server {
    fn new(
        target_sender: Sender<
            'static,
            CriticalSectionRawMutex,
            TargetRequest,
            REQUEST_CHANNEL_SIZE,
        >,
        response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
        rtt_rsp_ch: &'static Channel<CriticalSectionRawMutex, Result<RttResponse, RttError>, 1>,
        header_buf: &'static mut [u8; HTTPD_HEADER_BUF_SIZE],
        body_buf: &'static mut [u8; HTTPD_BODY_BUF_SIZE],
    ) -> Self {
        Self {
            target_sender,
            response_signal,
            rtt_rsp_ch,
            header_buf,
            body_buf,
        }
    }

    async fn handle_request(
        &mut self,
        socket: &mut TcpSocket<'_>,
    ) -> Result<Response, AirfrogError> {
        let mut response_format = None;

        // Read headers until we find \r\n\r\n
        let header_end;
        let mut total_read = 0;
        loop {
            if total_read >= HTTPD_HEADER_BUF_SIZE {
                info!("httpd: Header buffer overflow, request too large");
                return Ok(Response::status_code(
                    response_format,
                    None,
                    StatusCode::TooLarge,
                ));
            }

            let n = socket.read(&mut self.header_buf[total_read..]).await?;
            if n == 0 {
                if total_read == 0 {
                    debug!("httpd: Client dropped connection");
                    return Err(AirfrogError::Airfrog(ErrorKind::Network));
                } else {
                    info!("httpd: Connection closed during reading headers");
                }
                return Err(AirfrogError::Airfrog(ErrorKind::Network));
            }
            total_read += n;

            // Look for end of headers
            if let Some(pos) = self.header_buf[..total_read]
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
            {
                header_end = pos + 4;
                break;
            }
        }

        // Parse headers
        let mut headers = [httparse::EMPTY_HEADER; HTTPD_MAX_HEADERS];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(&self.header_buf[..header_end]) {
            Ok(_) => (),
            Err(e) => {
                info!("httpd: Failed to parse HTTP request: {e}");
                return Ok(Response::status_code(
                    response_format,
                    None,
                    StatusCode::BadRequest,
                ));
            }
        }

        // Parse method and path
        let (method, path) = match (req.method, req.path) {
            (Some(method_str), Some(path)) => {
                let method = Method::from_str(method_str);
                if method.is_none() {
                    info!("httpd: Failed to parse method {method_str}");
                    return Ok(Response::bad_request(response_format, path));
                }
                (method.unwrap(), path)
            }
            (None, _) => {
                info!("httpd: Failed to parse method");
                return Ok(Response::status_code(
                    response_format,
                    None,
                    StatusCode::BadRequest,
                ));
            }
            (Some(path), None) => {
                info!("httpd: Failed to parse path");
                return Ok(Response::not_found(response_format, path));
            }
        };

        // Find Content-Length if present
        let content_length = headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-length"))
            .and_then(|h| core::str::from_utf8(h.value).ok())
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        if content_length > HTTPD_BODY_BUF_SIZE {
            info!("httpd: Body too large");
            return Ok(Response::too_large(response_format, path));
        }

        let accept = headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("accept"))
            .and_then(|h| core::str::from_utf8(h.value).ok())
            .unwrap_or("");
        response_format = if accept.contains("application/json") {
            Some(ContentType::Json)
        } else if accept.contains("html") || accept.contains("*/*") {
            Some(ContentType::Html)
        } else {
            None
        };

        // Read body if present
        let body = if content_length > 0 {
            // May already have some body bytes after headers
            let already_read = total_read - header_end;
            let mut body_read = already_read.min(content_length);

            // Copy what we already have
            self.body_buf[..body_read]
                .copy_from_slice(&self.header_buf[header_end..header_end + body_read]);

            // Read remaining body
            while body_read < content_length {
                let n = socket
                    .read(&mut self.body_buf[body_read..content_length.min(HTTPD_BODY_BUF_SIZE)])
                    .await?;
                if n == 0 {
                    info!("httpd: Connection closed before body was fully read");
                    return Err(AirfrogError::Airfrog(ErrorKind::Network));
                }
                body_read += n;
            }

            match core::str::from_utf8(&self.body_buf[..content_length]) {
                Ok(body) => Some(body),
                Err(e) => {
                    info!("httpd: Failed to read request body: {e}");
                    return Ok(Response::bad_request(response_format, path));
                }
            }
        } else {
            None
        };

        // Route the request.  We do this in with_timeout() to handle, for
        // example, Target not responding because the binary API is active.
        match with_timeout(
            ROUTER_TIMEOUT,
            self.route_request(response_format, method, path, body),
        )
        .await
        {
            Ok(response) => Ok(response),
            Err(_) => {
                info!("httpd: Router timed out");
                Ok(Response::status_code(
                    response_format,
                    Some(path),
                    StatusCode::Timeout,
                ))
            }
        }
    }

    async fn route_request(
        &self,
        response_format: Option<ContentType>,
        method: Method,
        path: &str,
        body: Option<&str>,
    ) -> Response {
        trace!("httpd: Handle {method} {path}");

        // Handle special cases first
        match path {
            // Handle root and index routes
            "/" | "/index" | "/index.htm" | "/index.html" | "/index.php" | "/default.html"
            | "/default.htm" | "/home" | "/home/" | "/home.html" | "/home.htm" | "/main"
            | "/main/" | "/main.html" | "/airfrog" | "/airfrog/" => {
                return Response::redirect(path, "/www/");
            }
            // Requires special handling - can't be redirected
            "/favicon.ico" => {
                return Response {
                    path: Some(path.to_string()),
                    status_code: StatusCode::Ok,
                    content_type: Some(FAVICON.content_type),
                    content: Some(ResponseContent::Borrowed(FAVICON.content)),
                    headers: Some(vec![Header {
                        name: "Connection",
                        value: "close",
                    }]),
                };
            }
            _ => {}
        }

        // Handle statics
        for file in STATIC_FILES {
            if path == file.path {
                return Response::static_file(path, file);
            }
        }

        // Handle WWW routes
        if let Some(www_path) = path.strip_prefix("/www") {
            let response = match (method, www_path) {
                (Method::Get, "/" | "") => Response::html_ok(path, self.handle_dashboard().await),
                (Method::Get, "/browser") => Response::html_ok(path, self.handle_browser().await),
                (Method::Get, "/firmware") => Response::html_ok(path, self.handle_firmware().await),
                (Method::Get, "/target_update") => {
                    Response::html_ok(path, self.handle_target_update().await)
                }
                (Method::Get, "/rtt") => Response::html_ok(path, self.handle_www_rtt().await),
                (Method::Get, "/info") => Response::html_ok(path, self.handle_info().await),
                (Method::Get, "/settings") => Response::html_ok(path, self.handle_settings().await),
                _ => return Response::redirect(path, "/www/"),
            };
            return response.no_cache();
        }

        // Handle API routes
        if let Some(api_path) = path.strip_prefix("/api") {
            // Override the response format to JSON
            let response_format = Some(ContentType::Json);

            let (rest_type, clean_path) = if let Some(target_path) =
                api_path.strip_prefix("/target")
            {
                (Rest::Target, target_path)
            } else if let Some(raw_path) = api_path.strip_prefix("/raw") {
                (Rest::Raw, raw_path)
            } else if let Some(raw_path) = api_path.strip_prefix("/config/swd") {
                (Rest::SwdConfig, raw_path)
            } else if let Some(raw_path) = api_path.strip_prefix("/config/net") {
                return self
                    .rest_config_net(response_format, method, raw_path, body.map(String::from))
                    .await;
            } else if let Some(raw_path) = api_path.strip_prefix("/reboot") {
                return self.handle_reboot(response_format, path, raw_path, method);
            } else if let Some(raw_path) = api_path.strip_prefix("/rtt") {
                return self
                    .handle_rtt(
                        response_format,
                        path,
                        raw_path,
                        method,
                        body.map(String::from),
                    )
                    .await;
            } else {
                return Response::status_code(response_format, Some(path), StatusCode::NotFound);
            };

            // If we reach here, we need to send a command to the Target -
            // build it
            let cmd =
                match Command::from_rest(rest_type, method, clean_path, body.map(String::from)) {
                    Ok(cmd) => cmd,
                    Err(e) => return Response::error(response_format, path, e),
                };

            // Send it
            let response = self.handle_command(cmd).await;
            return Response::target_response(path, response).no_cache();
        }

        // If we reach here, the path was not found
        Response::not_found(response_format, path)
    }

    // Handles commands by sending to the Target and returning the response.
    // Returns a TargetResponse type which impls Into<Response>.
    async fn handle_command(&self, command: Command) -> TargetResponse {
        debug!("Command request: {command:?}");

        // Submit it to Target
        let request = TargetRequest {
            command,
            response_signal: self.response_signal,
        };
        self.target_sender.send(request).await;

        // Receive and return the response
        self.response_signal.wait().await
    }

    async fn get_summary_info(&self, output_fw_summary: bool) -> String {
        let response = self.send_command(Command::FirmwareInfo).await;
        html_summary(response.status, response.data, output_fw_summary)
    }
    /// Handles the dashboard route.
    async fn handle_dashboard(&self) -> HtmlContent {
        page_dashboard(self.get_summary_info(true).await)
    }

    async fn handle_browser(&self) -> HtmlContent {
        page_target_browser(self.get_summary_info(true).await)
    }

    async fn handle_firmware(&self) -> HtmlContent {
        let response = self.send_command(Command::FirmwareInfo).await;
        page_target_firmware(response)
    }

    async fn handle_target_update(&self) -> HtmlContent {
        page_target_update(self.get_summary_info(true).await)
    }

    async fn handle_www_rtt(&self) -> HtmlContent {
        page_target_rtt(self.get_summary_info(true).await)
    }

    async fn handle_info(&self) -> HtmlContent {
        // Get flash size here, as it requires async - avoids making page_info
        // async
        let flash_size_bytes = DEVICE.get().await.lock().await.flash_size_bytes();
        page_info(flash_size_bytes)
    }

    async fn handle_settings(&self) -> HtmlContent {
        let response = self.send_command(Command::FirmwareInfo).await;
        page_settings(response).await
    }

    async fn handle_rtt(
        &self,
        response_format: Option<ContentType>,
        path: &str,
        raw_path: &str,
        method: Method,
        _body: Option<String>,
    ) -> Response {
        match (method, raw_path) {
            (Method::Get, "/data") => {
                // Return all of the receive RTT data
                let cmd = RttCommand::Read { max: 256 };
                rtt_command(cmd, self.rtt_rsp_ch.sender()).await;
                match self.rtt_rsp_ch.receiver().receive().await {
                    Ok(rtt_rsp) => match rtt_rsp {
                        RttResponse::Data { data } => {
                            let hex_data: Vec<String> =
                                data.iter().map(|&b| format!("0x{:02X}", b)).collect();
                            let response_data = serde_json::json!({"data": hex_data});
                            Response::json(path, response_data, StatusCode::Ok)
                        }

                        _ => Response::status_code(
                            response_format,
                            Some(path),
                            StatusCode::InternalServerError,
                        ),
                    },
                    Err(e) => {
                        error!("Failed to get RTT data: {e:?}");
                        Response::status_code(
                            response_format,
                            Some(path),
                            StatusCode::InternalServerError,
                        )
                    }
                }
            }
            _ => Response::not_found(response_format, path),
        }
    }

    fn handle_reboot(
        &self,
        response_format: Option<ContentType>,
        path: &str,
        raw_path: &str,
        method: Method,
    ) -> Response {
        if raw_path.is_empty() && method == Method::Post {
            info!("Info:  Received /api/reboot POST request");
            REBOOT_SIGNAL.signal(());
            let mut response = Response::json(path, serde_json::json!({}), StatusCode::Ok);
            response.headers = Some(vec![Header {
                name: "Connection",
                value: "close",
            }]);
            return response;
        } else {
            return Response::status_code(response_format, Some(path), StatusCode::NotFound);
        }
    }

    async fn rest_config_net(
        &self,
        response_format: Option<ContentType>,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> Response {
        match method {
            Method::Get => {
                let config = CONFIG.get().await.lock().await;
                let json = config.net.serialize();
                Response::json(path, json, StatusCode::Ok)
            }
            Method::Post => {
                let json = match parse_json_body(body) {
                    Ok(Some(json)) => json,
                    Ok(None) => {
                        debug!("No body provided for network config update");
                        return Response::status_code(
                            response_format,
                            Some(path),
                            StatusCode::BadRequest,
                        );
                    }
                    Err(e) => {
                        debug!("Failed to parse network config JSON: {e}");
                        return Response::status_code(
                            response_format,
                            Some(path),
                            StatusCode::BadRequest,
                        );
                    }
                };

                let mut config = CONFIG.get().await.lock().await;
                match Net::deserialize(json) {
                    Some(net) => {
                        // Update network config
                        config.net = net;

                        // Store the new config to flash
                        config.update_flash().await;

                        Response::status_code(response_format, Some(path), StatusCode::Ok)
                    }
                    None => {
                        debug!("Failed to deserialize network config");
                        Response::status_code(response_format, Some(path), StatusCode::BadRequest)
                    }
                }
            }
        }
    }

    async fn send_command(&self, command: Command) -> TargetResponse {
        let request = TargetRequest {
            command,
            response_signal: self.response_signal,
        };
        self.target_sender.send(request).await;
        self.response_signal.wait().await
    }
}

/// Starts the HTTP server tasks, if the `httpd` feature is enabled.
pub(crate) async fn start(
    net_stack: Option<Stack<'static>>,
    target_sender: Sender<'static, CriticalSectionRawMutex, TargetRequest, REQUEST_CHANNEL_SIZE>,
    spawner: &Spawner,
) {
    if cfg!(feature = "httpd") {
        let net_stack = net_stack
            .as_ref()
            .expect("Internal error - WiFi stack not initialized");

        // We need to create the httpd statics for each task on separate
        // line.  A loop here would cause a make_static! panic, as the
        // "same" static would be initialized twice.  Start them
        // backwards (0 last), so they start in the obvious order.
        #[cfg(feature = "httpd")]
        const_assert_eq!(WEB_TASK_POOL_SIZE, 4);
        #[cfg(feature = "httpd")]
        const_assert_eq!(crate::NUM_SOCKETS_HTTPD, WEB_TASK_POOL_SIZE);
        let response_signal =
            make_static!(Signal::<CriticalSectionRawMutex, TargetResponse>::new());
        let header_buffer = make_static!([0; HTTPD_HEADER_BUF_SIZE]);
        let body_buffer = make_static!([0; HTTPD_BODY_BUF_SIZE]);
        let rtt_rsp_ch = make_static!(Channel::new());
        let server = make_static!(Server::new(
            target_sender,
            response_signal,
            rtt_rsp_ch,
            header_buffer,
            body_buffer
        ));
        spawner.must_spawn(task(3, *net_stack, server));

        let response_signal =
            make_static!(Signal::<CriticalSectionRawMutex, TargetResponse>::new());
        let header_buffer = make_static!([0; HTTPD_HEADER_BUF_SIZE]);
        let body_buffer = make_static!([0; HTTPD_BODY_BUF_SIZE]);
        let rtt_rsp_ch = make_static!(Channel::new());
        let server = make_static!(Server::new(
            target_sender,
            response_signal,
            rtt_rsp_ch,
            header_buffer,
            body_buffer
        ));
        spawner.must_spawn(task(2, *net_stack, server));

        let response_signal =
            make_static!(Signal::<CriticalSectionRawMutex, TargetResponse>::new());
        let header_buffer = make_static!([0; HTTPD_HEADER_BUF_SIZE]);
        let body_buffer = make_static!([0; HTTPD_BODY_BUF_SIZE]);
        let rtt_rsp_ch = make_static!(Channel::new());
        let server = make_static!(Server::new(
            target_sender,
            response_signal,
            rtt_rsp_ch,
            header_buffer,
            body_buffer
        ));
        spawner.must_spawn(task(1, *net_stack, server));

        let response_signal =
            make_static!(Signal::<CriticalSectionRawMutex, TargetResponse>::new());
        let header_buffer = make_static!([0; HTTPD_HEADER_BUF_SIZE]);
        let body_buffer = make_static!([0; HTTPD_BODY_BUF_SIZE]);
        let rtt_rsp_ch = make_static!(Channel::new());
        let server = make_static!(Server::new(
            target_sender,
            response_signal,
            rtt_rsp_ch,
            header_buffer,
            body_buffer
        ));
        spawner.must_spawn(task(0, *net_stack, server));
    } else {
        info!("Note:  HTTP server not started - 'httpd' feature not enabled");
    }
}

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
async fn task(id: usize, stack: embassy_net::Stack<'static>, server: &'static mut Server) -> ! {
    let port = HTTPD_PORT;
    let ip = stack
        .config_v4()
        .expect("Web server - failed to get IPv4 config")
        .address
        .address();

    info!("Exec:  HTTPD {id} task started on {ip}:{port}");

    let mut rx_buffer = [0; HTTPD_TASK_TCP_RX_BUF_SIZE];
    let mut tx_buffer = [0; HTTPD_TASK_TCP_TX_BUF_SIZE];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        if let Err(e) = socket.accept(HTTPD_PORT).await {
            warn!("httpd: Task {id} server accept error: {e:?}");
            continue;
        }

        // Log connection once when established
        if let Some(edpt) = socket.remote_endpoint().as_ref() {
            info!("httpd: Task {id} connection from {}", edpt.addr);
        } else {
            warn!("httpd: Task {id} connection from unknown address");
        }

        // Handle multiple requests on this connection
        let _ = loop {
            match server.handle_request(&mut socket).await {
                Ok(rsp) => {
                    trace!("httpd: Task {id} Response {rsp}");
                    if let Err(e) = rsp.write_to(&mut socket).await {
                        // Write failed - close the connection
                        break e.into();
                    }
                    // Continue to next request on same connection
                }
                Err(e) => {
                    // Connection closed, timeout, or parse error
                    break e;
                }
            }
        };
        info!("httpd: Task {id} connection closed");

        // Explicitly close the socket after the connection has errored out
        socket.close();
    }
}
