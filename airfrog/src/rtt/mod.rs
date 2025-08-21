// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - RTT objects and routines

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use embassy_futures::select::{Either, Either3, select, select3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_sync::signal::Signal;
use embassy_time::Timer;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use static_cell::make_static;

use crate::target::{
    Command as TargetCommand, REQUEST_CHANNEL_SIZE, Request as TargetRequest,
    Response as TargetResponse,
};

static RTT_CMD_CHANNEL: Channel<
    CriticalSectionRawMutex,
    (
        Command,
        Option<Sender<CriticalSectionRawMutex, Result<Response, Error>, 1>>,
    ),
    2,
> = Channel::new();
static RTT_CTRL_SIGNAL: Signal<CriticalSectionRawMutex, Control> = Signal::new();

const BUFFER_SIZE: usize = 4096;

const MAX_BYTES_PER_READ: usize = 256;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum State {
    Start,
    Stop,
}

#[derive(PartialEq, Eq)]
pub enum Control {
    /// Start the RTT task.
    Start { rtt_cb_loc: u32 },

    /// Stop the RTT task.
    Stop,
}

impl core::fmt::Debug for Control {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Control::Start { rtt_cb_loc } => write!(f, "Start(rtt_cb_loc: {rtt_cb_loc:#010X})"),
            Control::Stop => write!(f, "Stop"),
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum Command {
    /// Retrieves the RTT state.
    /// Returns:
    /// - `Ok(Response::State)` if successful
    /// - `Err(Error)` on failure
    _GetState,

    /// Reads up to `max` bytes from the RTT buffer.
    /// Returns:
    /// - `Ok(Response::Data)` if successful
    /// - `Err(Error)` on failure
    Read { max: usize },
}

impl core::fmt::Debug for Command {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Command::_GetState => write!(f, "GetState"),
            Command::Read { max } => write!(f, "Read(max: {max})"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    /// The current state of the RTT task.
    State { state: State },

    /// The data read from the RTT buffer.
    Data { data: Vec<u8> },
}

#[derive(Debug)]
pub enum Error {
    // Cannot complete the command due to Rtt's state.
    Stopped,

    // No data to return.
    NoData,

    // Cannot find RTT data at the indicated location
    NotFound,

    // Error from target
    Target,

    // Internal error
    Internal,

    // Our local buffer is full
    Full,
}

impl From<serde_json::Error> for Error {
    fn from(_error: serde_json::Error) -> Self {
        Error::Target
    }
}

// Representation of the main RTT control block
struct RttCb {
    location: u32,
    max_up_bufs: i32,
    _max_down_bufs: i32,
}

impl RttCb {
    const HEADER_SIZE: usize = 24;

    fn from_location_bytes(location: u32, bytes: &[u8]) -> Result<Self, Error> {
        // - 16 bytes "SEGGER RTT\0\0\0\0\0\0"
        // - 4 bytes (signed) MaxNumUpBuffers
        // - 4 bytes (signed) MaxNumDownBuffers

        if bytes.len() < Self::HEADER_SIZE {
            return Err(Error::Internal);
        }

        if bytes[0..16] != *b"SEGGER RTT\0\0\0\0\0\0" {
            info!("Info:  No SEGGER RTT header found");
            debug!("Info:  Bytes: {:?}", &bytes[0..16]);
            return Err(Error::NotFound);
        }

        let max_up_bufs = i32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let max_down_bufs = i32::from_le_bytes(bytes[20..24].try_into().unwrap());

        Ok(Self {
            location,
            max_up_bufs,
            _max_down_bufs: max_down_bufs,
        })
    }

    fn get_up_buf_loc(&self, index: usize) -> Result<u32, Error> {
        if index > self.max_up_bufs as usize {
            return Err(Error::NotFound);
        }
        let up_buf_start = self.location + Self::HEADER_SIZE as u32;
        let up_buf_loc = up_buf_start + (index * RttBuf::SIZE) as u32;
        Ok(up_buf_loc)
    }

    fn _get_down_buf_loc(&self, index: usize) -> Result<u32, Error> {
        if index > self._max_down_bufs as usize {
            return Err(Error::NotFound);
        }
        let down_buf_start = self.location + (self.max_up_bufs as usize * RttBuf::SIZE) as u32;
        let down_buf_loc = down_buf_start + (index * RttBuf::SIZE) as u32;
        Ok(down_buf_loc)
    }
}

// An up or down RTT buffer
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
struct RttBuf {
    location: u32,
    data_ptr: u32,
    name_ptr: u32,
    size: u32,
    write_pos: u32,
    read_pos: u32,
}

impl RttBuf {
    const SIZE: usize = 24;
    const WRITE_POS_OFFSET: usize = 12;
    const READ_POS_OFFSET: usize = 16;

    fn from_location_bytes(location: u32, bytes: &[u8]) -> Result<Self, Error> {
        // - 4 byte pointer to the buffer's name
        // - 4 byte pointer to the start of the buffer
        // - 4 byte unsigned size of the buffer
        // - 4 byte unsigned (target) write offset
        // - 4 byte unsigned (host, our) read offset
        // - 4 byte flags - check top byte is zero.

        if bytes.len() < Self::SIZE {
            return Err(Error::Internal);
        }

        let name_ptr = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let data_ptr = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let write_pos = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let read_pos = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let flags = u32::from_le_bytes(bytes[20..24].try_into().unwrap());

        if flags & 0xFF000000 != 0 {
            info!("Invalid RTT buffer flags {flags:#010X}");
            return Err(Error::NotFound);
        }

        Ok(Self {
            location,
            data_ptr,
            name_ptr,
            size,
            write_pos,
            read_pos,
        })
    }

    fn cur_read_loc(&self) -> u32 {
        self.data_ptr + self.read_pos
    }

    fn write_pos_field_loc(&self) -> u32 {
        self.location + Self::WRITE_POS_OFFSET as u32
    }

    fn read_pos_field_loc(&self) -> u32 {
        self.location + Self::READ_POS_OFFSET as u32
    }

    fn available_data(&self) -> usize {
        if self.write_pos > self.read_pos {
            (self.write_pos - self.read_pos) as usize
        } else {
            ((self.size - self.read_pos) + self.write_pos) as usize
        }
    }
}

// Our (Airfrog's) copy of the RTT data
struct LocalBuf {
    data: [u8; BUFFER_SIZE],
    write_pos: usize,
    read_pos: usize,
}

impl Default for LocalBuf {
    fn default() -> Self {
        Self {
            data: [0; BUFFER_SIZE],
            write_pos: 0,
            read_pos: 0,
        }
    }
}

impl LocalBuf {
    fn available_space(&self) -> usize {
        if self.read_pos > self.write_pos {
            self.read_pos - self.write_pos - 1
        } else {
            BUFFER_SIZE - (self.write_pos - self.read_pos) - 1
        }
    }

    fn available_data(&self) -> usize {
        if self.write_pos >= self.read_pos {
            self.write_pos - self.read_pos
        } else {
            BUFFER_SIZE - self.read_pos + self.write_pos
        }
    }

    fn write_data(&mut self, data: &[u8]) {
        for &byte in data {
            self.data[self.write_pos] = byte;
            self.write_pos = (self.write_pos + 1) % BUFFER_SIZE;
        }
    }

    fn read_data(&mut self, buf: &mut [u8]) -> usize {
        let available = self.available_data();
        let to_read = core::cmp::min(buf.len(), available);

        if self.read_pos + to_read <= BUFFER_SIZE {
            // Contiguous read
            buf[..to_read].copy_from_slice(&self.data[self.read_pos..self.read_pos + to_read]);
        } else {
            // Split read
            let first_part = BUFFER_SIZE - self.read_pos;
            let second_part = to_read - first_part;

            buf[..first_part].copy_from_slice(&self.data[self.read_pos..]);
            buf[first_part..to_read].copy_from_slice(&self.data[..second_part]);
        }

        self.read_pos = (self.read_pos + to_read) % BUFFER_SIZE;
        to_read
    }
}

struct Rtt {
    state: State,
    rtt_cb: Option<RttCb>,
    rtt_up_buf: Option<RttBuf>,
    local_buf: LocalBuf,
    target_sender: Sender<'static, CriticalSectionRawMutex, TargetRequest, REQUEST_CHANNEL_SIZE>,
    target_response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
}

impl Rtt {
    fn new(
        target_sender: Sender<
            'static,
            CriticalSectionRawMutex,
            TargetRequest,
            REQUEST_CHANNEL_SIZE,
        >,
        target_response_signal: &'static Signal<CriticalSectionRawMutex, TargetResponse>,
    ) -> Self {
        Self {
            state: State::Stop,
            rtt_cb: None,
            rtt_up_buf: None,
            local_buf: LocalBuf::default(),
            target_sender,
            target_response_signal,
        }
    }

    // Reads bytes from the target
    async fn read_bytes(&mut self, location: u32, num_bytes: usize) -> Result<Vec<u8>, Error> {
        if num_bytes == 0 || !num_bytes.is_multiple_of(4) {
            warn!("Error: Internal error requested invalid number of bytes {num_bytes}");
            return Err(Error::Internal);
        }

        // Build and send the request
        let command = TargetCommand::ReadMemBulk {
            addr: format!("{location:#010X}"),
            count: num_bytes / 4,
        };
        let response_signal = self.target_response_signal;
        let request = TargetRequest {
            command,
            response_signal,
        };
        self.target_sender.send(request).await;

        // Wait for the response
        let rsp = response_signal.wait().await;

        // Turn the response into a Vec of bytes
        let data = rsp.data.ok_or(Error::NoData)?;
        let hex_strings: Vec<String> = serde_json::from_value(data)?;
        let mut bytes = Vec::new();
        for hex_str in hex_strings {
            let word = u32::from_str_radix(hex_str.trim_start_matches("0x"), 16)
                .map_err(|_| Error::Internal)?;
            bytes.extend_from_slice(&word.to_le_bytes());
        }

        // Return it
        Ok(bytes)
    }

    async fn read_word(&mut self, location: u32) -> Result<u32, Error> {
        // Read 4 bytes from the target
        let bytes = self.read_bytes(location, 4).await?;
        if bytes.len() != 4 {
            return Err(Error::NoData);
        }
        Ok(u32::from_le_bytes(bytes[0..4].try_into().unwrap()))
    }

    async fn write_word(&mut self, location: u32, word: u32) -> Result<(), Error> {
        // Build and send the request
        let command = TargetCommand::WriteMem {
            addr: format!("{location:#010X}"),
            data: format!("{word:#010X}"),
        };
        let response_signal = self.target_response_signal;
        let request = TargetRequest {
            command,
            response_signal,
        };
        self.target_sender.send(request).await;

        // Wait for the response
        let rsp = response_signal.wait().await;

        // Check for error
        if rsp.error.is_some() {
            return Err(Error::Target);
        }

        Ok(())
    }

    async fn get_rtt_cb(&mut self, location: u32) -> Result<RttCb, Error> {
        // Query the RTT CB from RAM
        let bytes = self.read_bytes(location, RttCb::HEADER_SIZE).await?;
        RttCb::from_location_bytes(location, &bytes)
    }

    async fn get_rtt_up_buf(&mut self, rtt_cb: &RttCb, index: usize) -> Result<RttBuf, Error> {
        // Query the first RTT Up Buffer from RAM
        let up_buf_loc = rtt_cb.get_up_buf_loc(index)?;
        let bytes = self.read_bytes(up_buf_loc, RttBuf::SIZE).await?;
        RttBuf::from_location_bytes(up_buf_loc, &bytes)
    }

    // Queries and parses the RTT control block, and initializes read and
    // write positions
    async fn start(&mut self, location: u32) -> Result<(), Error> {
        // Query the RTT CB from RAM.  This is a two stage process:
        //
        // Query 24 bytes from the start of SEGGER_RTT_CB:
        // - 16 bytes "SEGGER RTT\0\0\0\0\0\0"
        // - 4 bytes (signed) MaxNumUpBuffers
        // - 4 bytes (signed) MaxNumDownBuffers
        //
        // Query 24 bytes from the first SEGGER_RTT_BUFFER_UP:
        // - 4 byte pointer to the buffer's name
        // - 4 byte pointer to the start of the buffer
        // - 4 byte unsigned size of the buffer
        // - 4 byte unsigned (target) write offset
        // - 4 byte unsigned (host, our) read offset
        // - 4 byte flags - check top byte is zero.
        //
        // Strictly, as we're only going to read the first up buffer, we could
        // just read it as part of the first read - as it directly follows the
        // first 24 bytes, but we'll do it "properly" for easier future
        // extension.
        let rtt_cb = self.get_rtt_cb(location).await?;
        let rtt_up_buf = self.get_rtt_up_buf(&rtt_cb, 0).await?;

        // Only store the CB/buf now we've succeeded
        self.rtt_cb = Some(rtt_cb);
        self.rtt_up_buf = Some(rtt_up_buf);
        if self.state != State::Start {
            // If we're not already started, set the state to Start
            info!("Info:  RTT started");
            self.state = State::Start;
        } else {
            debug!("Info:  RTT re-started at {location:#010X}");
        }

        Ok(())
    }

    fn stop(&mut self) {
        self.rtt_cb = None;
        self.rtt_up_buf = None;
        self.state = State::Stop;

        info!("Info:  RTT stopped");
    }

    async fn handle_command(
        &mut self,
        command: Command,
        sender: Option<Sender<'static, CriticalSectionRawMutex, Result<Response, Error>, 1>>,
    ) {
        let response = match command {
            Command::_GetState => Ok(Response::State { state: self.state }),
            Command::Read { max } => match self.state {
                State::Stop => Err(Error::Stopped),
                State::Start => {
                    let max = core::cmp::min(max, MAX_BYTES_PER_READ);
                    let mut data = vec![0u8; max];
                    let bytes_read = self.local_buf.read_data(&mut data);
                    Ok(Response::Data {
                        data: data[0..bytes_read].to_vec(),
                    })
                }
            },
        };

        if let Some(sender) = sender {
            sender.send(response).await;
        }
    }

    // Function to get any new RTT data from the target.
    async fn get_rtt_data_from_target(&mut self) -> Result<(), Error> {
        if self.local_buf.available_space() == 0 {
            // No space in our local buffer, so no point reading any more data
            return Err(Error::Full);
        }

        let mut stored_buf = self.rtt_up_buf.ok_or(Error::Stopped)?.clone();

        // Get current read position
        let cur_read_pos = self.read_word(stored_buf.read_pos_field_loc()).await?;
        if cur_read_pos != stored_buf.read_pos {
            // If the read position has changed, restart
            info!("Info:  RTT task detected potential device reset - RTT read position changed from {} to {}", stored_buf.read_pos, cur_read_pos);
            let location = self.rtt_cb.as_ref().ok_or(Error::Stopped)?.location;
            self.stop();
            self.start(location).await?;
            stored_buf = self.rtt_up_buf.ok_or(Error::Stopped)?.clone();
        }

        // Get current write position
        stored_buf.write_pos = self.read_word(stored_buf.write_pos_field_loc()).await?;
        trace!(
            "Info:  RTT write/read positions: {} {}",
            stored_buf.write_pos, stored_buf.read_pos
        );

        // No data? Done.
        if stored_buf.write_pos == stored_buf.read_pos {
            return Ok(());
        }

        // Calculate available data and limit to MAX_BYTES_PER_READ bytes
        let target_available = stored_buf.available_data();
        let space = self.local_buf.available_space();
        let max_read = core::cmp::min(core::cmp::min(target_available, space), MAX_BYTES_PER_READ);
        let max_read_aligned = max_read & !3;

        if max_read_aligned == 0 {
            return Ok(());
        }

        // Read only the data we need (handle wraparound)
        let available_data = if stored_buf.write_pos > stored_buf.read_pos {
            // Contiguous read
            let bytes_to_read = core::cmp::min(
                max_read_aligned,
                (stored_buf.write_pos - stored_buf.read_pos) as usize,
            );
            self.read_bytes(stored_buf.cur_read_loc(), bytes_to_read)
                .await?
        } else {
            // Wrapped read
            let first_chunk = (stored_buf.size - stored_buf.read_pos) as usize;
            if max_read_aligned <= first_chunk {
                // All data fits in first chunk
                self.read_bytes(stored_buf.cur_read_loc(), max_read_aligned)
                    .await?
            } else {
                // Need both chunks
                let mut data = self
                    .read_bytes(stored_buf.cur_read_loc(), first_chunk)
                    .await?;
                let second_chunk = max_read_aligned - first_chunk;
                let mut second_data = self.read_bytes(stored_buf.data_ptr, second_chunk).await?;
                data.append(&mut second_data);
                data
            }
        };

        debug!("Info:  RTT data available: {} bytes", available_data.len());

        // Copy what we can fit
        let space = self.local_buf.available_space();
        let to_copy = core::cmp::min(available_data.len(), space);

        if to_copy > 0 {
            self.local_buf.write_data(&available_data[..to_copy]);

            // Update read position by how much we actually consumed
            stored_buf.read_pos = (stored_buf.read_pos + to_copy as u32) % stored_buf.size;
            self.write_word(stored_buf.read_pos_field_loc(), stored_buf.read_pos)
                .await?;
            self.rtt_up_buf = Some(stored_buf);
        }

        Ok(())
    }
}

#[embassy_executor::task]
pub async fn rtt_task(
    target_sender: Sender<'static, CriticalSectionRawMutex, TargetRequest, REQUEST_CHANNEL_SIZE>,
) {
    let signal = Signal::new();
    let signal = make_static!(signal);
    let mut rtt = Rtt::new(target_sender, signal);

    // While this looks like a tight loop, it's yielding in all cases,
    // ensuring no other tasks get starved.  If this is a bit overwhelming for
    // a Target, we can add a delay after reading data before reading again.
    loop {
        // Wait for a command, and, if we're running, data from the target
        let command = if rtt.state == State::Stop {
            // When stopped, listen for both data commands (to reject) and control (to start)
            match select(RTT_CMD_CHANNEL.receive(), RTT_CTRL_SIGNAL.wait()).await {
                Either::First(cmd) => Some(cmd),
                Either::Second(Control::Start { rtt_cb_loc }) => {
                    debug!("Info:  Received RTT start command at {rtt_cb_loc:#010X}");
                    rtt.start(rtt_cb_loc)
                        .await
                        .inspect_err(|e| error!("Error: Failed to start RTT: {e:?}"))
                        .ok();
                    None
                }
                Either::Second(Control::Stop) => None,
            }
        } else {
            if let Err(e) = rtt.get_rtt_data_from_target().await {
                info!("Error: Hit error receiving RTT data {e:?}");
            }

            match select3(
                RTT_CMD_CHANNEL.receive(),
                RTT_CTRL_SIGNAL.wait(),
                Timer::after_millis(100),
            )
            .await
            {
                Either3::First(command) => Some(command),
                Either3::Second(control) => match control {
                    Control::Start { rtt_cb_loc } => {
                        debug!("Info:  Received RTT start command at {rtt_cb_loc:#010X}");
                        rtt.start(rtt_cb_loc)
                            .await
                            .inspect_err(|e| {
                                error!("Error: Failed to start RTT: {e:?}");
                            })
                            .ok();
                        None
                    }
                    Control::Stop => {
                        debug!("Info:  Received RTT stop command");
                        rtt.stop();
                        None
                    }
                },
                Either3::Third(_) => {
                    // Timeout, just continue the loop
                    None
                }
            }
        };

        // Handle the command if there is one
        if let Some((command, sender)) = command {
            trace!("Info:  Handle RTT command {command:?}");
            rtt.handle_command(command, sender).await;
        }
    }
}

/// Helper function to send a command to the RTT and receive the response.
///
/// Arguments:
/// - `command`: The command to send to the RTT.
/// - `response_channel`: The channel to receive the response.
///
/// Returns:
/// - The response from the RTT task.
pub async fn rtt_command(
    command: Command,
    response_sender: Sender<'static, CriticalSectionRawMutex, Result<Response, Error>, 1>,
) {
    RTT_CMD_CHANNEL.send((command, Some(response_sender))).await
}

pub fn rtt_control(control: Control) {
    RTT_CTRL_SIGNAL.signal(control);
}
