// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog-swd - Binary API for the Airfrog SWD target
//!
//! See [`Binary API`](https://github.com/piersfinlayson/airfrog/blob/main/docs/REST-API.md)
//! for the binary API specification.

#![allow(unused)]

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, vec};
use embassy_net::tcp::{Error as TcpError, State, TcpReader, TcpSocket};
use embassy_time::{Duration, Timer};
use embedded_io::BufRead;
use embedded_io::ReadExactError;
use embedded_io_async::Read;
use embedded_io_async::Write;
use esp_hal::rtc_cntl::Swd;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use crate::debug::DebugInterface;
use crate::protocol::{LineState, Version};
use crate::{SwdError, with_timeout, with_timeout_no_return};
use airfrog_bin::{
    AsyncReader, LineLevel, Op, ParseError, ProtocolError, RegType, ResponseCode, Speed,
};
use airfrog_bin::{PORT, VERSION};
use airfrog_bin::{RSP_ERR_API, RSP_ERR_CMD, RSP_ERR_NET, RSP_ERR_SWD, RSP_ERR_TIMEOUT, RSP_OK};

// Binary API is closed if no activity in this time
const API_TIMEOUT: Duration = Duration::from_secs(120);

// Log every X binary API calls
const API_CALL_LOG_INTERVAL: usize = 1000;

struct ApiReader<'a>(&'a mut TcpReader<'a>);
impl AsyncReader for ApiReader<'_> {
    type Error = ReadExactError<TcpError>;

    fn read_exact(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<(), Self::Error>> {
        self.0.read_exact(buf)
    }
}

/// Represents a binary API instance.
///
/// Create, then call [`Self::serve()`] to start serving binary API requests.
/// You can do this within an existing task (in which case `serve()` takes
/// over the task, until the connection is closed), or you can spawn a new
/// task to handle the binary API connection.
pub struct Api {
    /// The number of binary API calls handled in this instance.
    pub binary_api_calls: usize,
}

impl Default for Api {
    fn default() -> Self {
        Api::new()
    }
}

impl Api {
    /// Creates a new instance of the binary API.
    fn new() -> Self {
        Api {
            binary_api_calls: 0,
        }
    }

    // Main function to serve binary API:
    // - Calls the main loop
    // - When the handler returns it logs and closes the socket
    pub async fn serve(&mut self, swd: &mut DebugInterface<'_>, socket: &mut TcpSocket<'_>) {
        let remote = socket.remote_endpoint().map(|ep| (ep.addr, ep.port));
        let remote_str = remote
            .map(|(ip, port)| format!("{ip}:{port}"))
            .unwrap_or_else(|| "unknown".to_string());
        info!("Exec:  Binary API connection from {remote_str}");

        match self.binary_api_main_loop(swd, socket).await {
            Ok(Some(api_response)) => {
                // Not much we can do if this fails
                let _ =
                    with_timeout_no_return!(API_TIMEOUT, socket.write_all(&[api_response as u8]));
            }
            Ok(None) => {}
            Err(e) => warn!("Error: Binary API exited {e}"),
        }

        if socket.state() != State::Closed {
            debug!("Exec:  Closing binary API socket");
            socket.close();
            Timer::after_millis(5).await;
        }

        // Just in case
        socket.abort();
        info!("Note:  Binary API shutdown");

        info!(
            "Note:  Binary API successfully handled {} calls this connection",
            self.binary_api_calls
        );
    }

    // Performs the binary API handshake:
    // - Sends a version byte
    // - Reads the version ack
    async fn binary_api_handshake(
        &mut self,
        swd: &mut DebugInterface<'_>,
        socket: &mut TcpSocket<'_>,
    ) -> Result<(), SwdError> {
        debug!("Exec:  Binary API handshake");

        // Send version byte
        if let Err(e) = with_timeout!(API_TIMEOUT, socket.write_all(&[VERSION])) {
            warn!("Error: Binary API failed to send version: {e:?}");
            return Err(SwdError::Api);
        }

        // Read version ack
        let mut version_buf = [0u8; 1];
        match with_timeout!(API_TIMEOUT, socket.read(&mut version_buf)) {
            Ok(count) => {
                if count == 0 {
                    warn!("Error: Binary API failed to read version ack: 0 bytes");
                    return Err(SwdError::Api);
                }
                if version_buf[0] == VERSION {
                    debug!("OK:    Binary API handshake complete");
                    Ok(())
                } else {
                    warn!(
                        "Error: Binary API Version mismatch: got 0x{:02X}, expected 0x01",
                        version_buf[0]
                    );
                    Err(SwdError::Api)
                }
            }
            Err(e) => {
                warn!("Error: Binary API failed to read version ack: {e:?}");
                Err(SwdError::Api)
            }
        }
    }

    // Serves the binary API.
    //
    // The loop continues until the API hits an error, or the client
    // disconnects.  The loop itself exits if this function needs to send an
    // error response back to the client.  Or it returns directly if a network
    // error occurs (and therefore there's no point trying to send a response).
    //
    // In either case, once this function has returned, the caller will close
    // the socket.
    async fn binary_api_main_loop(
        &mut self,
        swd: &mut DebugInterface<'_>,
        socket: &mut TcpSocket<'_>,
    ) -> Result<Option<ResponseCode>, SwdError> {
        self.binary_api_handshake(swd, socket).await?;

        let (mut reader, mut writer) = socket.split();
        let mut api_reader = ApiReader(&mut reader);

        // The loop exists if the API hits a (non-network) error.  The loop
        // returns an API response byte, which is then sent back to the client.
        // If a network error occurs, the loop returns directly back to the
        // calling function
        loop {
            // Read a command byte
            let mut cmd = [0u8; 1];
            let command =
                with_timeout!(API_TIMEOUT, Op::recv_cmd(&mut api_reader)).map_err(|e| {
                    warn!("Error: Binary API socket read failure {e:?}");
                    SwdError::Network
                })?;

            // Get the entire operation
            let op = {
                let result = Op::async_recv(command, &mut api_reader).await;
                if let Err(e) = result {
                    match e {
                        ParseError::Protocol(ProtocolError::Command(cmd)) => unreachable!(
                            "Internal error in binary API - command should have been parsed already: {cmd:04X}"
                        ),
                        ParseError::Protocol(ProtocolError::Arg) => {
                            warn!("Error: Binary API received invalid argument on cmd {command}");
                            break Ok(Some(ResponseCode::Api));
                        }
                        ParseError::Transport(e) => {
                            warn!("Error: Binary API failed to read/parse command {command} {e:?}");
                            return Err(SwdError::Network);
                        }
                    }
                } else {
                    result.unwrap()
                }
            };

            // If the command is a disconnect, we return OK
            if let Op::Disconnect = op {
                info!("Exec:  Binary API received disconnect command");
                break Ok(Some(ResponseCode::Ok));
            }

            // Handle the command.
            //
            // On success, we send back a RSP_OK, plus any additional data.
            // On failure, we send back an error response.
            trace!("Exec:  Binary API operation {op:?}");
            let result = self.binary_api_handle_op(swd, op).await;
            let (rsp, data) = match result {
                Ok(data) => (ResponseCode::Ok, data),
                Err(e) => {
                    warn!("Error: Binary API command {command} failed: {e:?}");
                    match e {
                        SwdError::WaitAck
                        | SwdError::NoAck(_)
                        | SwdError::FaultAck
                        | SwdError::ReadParity
                        | SwdError::DpError
                        | SwdError::OperationFailed(_)
                        | SwdError::NotReady => (ResponseCode::Swd, None),
                        SwdError::Network => (ResponseCode::Net, None),
                        SwdError::Timeout => (ResponseCode::Timeout, None),
                        SwdError::Api | SwdError::Unsupported => (ResponseCode::Api, None),
                    }
                }
            };

            // Send an OK response
            trace!("Exec:  Binary API sending response {rsp}");
            with_timeout!(API_TIMEOUT, writer.write_all(&[rsp as u8]))?;

            // Now send back any data
            if let Some(data) = data {
                trace!(
                    "Exec:  Binary API sending response data len: {}",
                    data.len()
                );
                with_timeout!(API_TIMEOUT, writer.write_all(&data))?;
            }

            // Log every so often
            self.binary_api_calls += 1;
            if self.binary_api_calls > 0
                && self.binary_api_calls.is_multiple_of(API_CALL_LOG_INTERVAL)
            {
                info!(
                    "Note:  Binary API handled {} calls so far this connection",
                    self.binary_api_calls
                );
            }
        }
    }

    fn binary_api_response_from_swd_error(e: SwdError) -> ResponseCode {
        match e {
            SwdError::Api | SwdError::Unsupported => ResponseCode::Api,
            SwdError::Network => ResponseCode::Net,
            SwdError::Timeout => ResponseCode::Timeout,
            _ => ResponseCode::Swd,
        }
    }

    // Handles a single binary API operation
    //
    // Returns true if the connection should be closed
    async fn binary_api_handle_op(
        &mut self,
        swd: &mut DebugInterface<'_>,
        op: Op,
    ) -> Result<Option<Vec<u8>>, SwdError> {
        match op {
            Op::DpRead { reg } => swd
                .swd_if()
                .read_dp_register_raw(reg)
                .await
                .map(|data| Some(data.to_le_bytes().to_vec())),
            Op::DpWrite { reg, data } => swd
                .swd_if()
                .write_dp_register_raw(reg, data)
                .await
                .map(|()| None),
            Op::ApRead { reg } => swd
                .swd_if()
                .read_ap_register_raw(0, reg)
                .await
                .map(|data| Some(data.to_le_bytes().to_vec())),
            Op::ApWrite { reg, data } => swd
                .swd_if()
                .write_ap_register_raw(0, reg, data)
                .await
                .map(|()| None),
            Op::ApBulkRead { reg, count } => {
                // Set auto-increment mode
                swd.swd_if().set_addr_inc(true).await?;

                // If a partial read succeeds throw away that data for
                // simplicity
                let mut buf = vec![0u32; count as usize];
                swd.swd_if()
                    .read_ap_register_raw_bulk(0, reg, &mut buf)
                    .await
                    .map_err(|(e, _)| e)?;

                // Build the response
                let mut response = Vec::with_capacity(2 + (count as usize * 4));
                response.extend(count.to_le_bytes());
                response.extend(buf.iter().flat_map(|word| word.to_le_bytes()));
                Ok(Some(response))
            }
            Op::ApBulkWrite { reg, data } => {
                let count = data.len();
                // Set auto-increment mode
                swd.swd_if().set_addr_inc(true).await?;

                // If we get a partial write error, ignore and just sent the
                // error response.  We're just going to close the connection
                // anyway.
                swd.swd_if()
                    .write_ap_register_raw_bulk(0, reg, &data)
                    .await
                    .map(|()| None)
                    .map_err(|(e, _)| e)
            }
            Op::MultiRegWrite { count, data } => {
                for reg_write in data {
                    let reg = reg_write.reg;
                    let word = reg_write.data;
                    match reg_write.reg_type {
                        RegType::Dp => {
                            swd.swd_if().write_dp_register_raw(reg, word).await?;
                        }
                        RegType::Ap => {
                            swd.swd_if().write_ap_register_raw(0, reg, word).await?;
                        }
                    }
                }
                Ok(None)
            }
            Op::Ping => Ok(None),
            Op::ResetTarget => {
                // Try to connect as V1.  If that fails, try V2.  Multi-drop
                // is not attempted.
                if swd.swd_if().reset_target(Version::V1).await.is_ok() {
                    Ok(None)
                } else {
                    swd.swd_if().reset_target(Version::V2).await.map(|_| None)
                }
            }
            Op::Clock {
                level,
                post_level,
                cycles,
            } => {
                let level = into_swd_line_state(level);
                let post = into_swd_line_state(post_level);
                swd.swd_if().clock_raw(level, post, cycles as u32);
                Ok(None)
            }
            Op::SetSpeed { speed } => {
                swd.swd_if().set_swd_speed(speed.into());
                Ok(None)
            }
            Op::Disconnect => {
                unreachable!("Binary API Disconnect should be handled in the main loop");
                Ok(None)
            }
        }
    }
}

fn from_swd_line_state(state: LineState) -> LineLevel {
    match state {
        LineState::High => LineLevel::High,
        LineState::Low => LineLevel::Low,
        LineState::Input => LineLevel::Input,
    }
}
fn into_swd_line_state(level: LineLevel) -> LineState {
    match level {
        LineLevel::High => LineState::High,
        LineLevel::Low => LineState::Low,
        LineLevel::Input => LineState::Input,
    }
}
