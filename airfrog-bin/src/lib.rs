// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! Airfrog is the tiny wireless co-processor for ARM.
//!
//! <https://piers.rocks/u/airfrog>
//!
//! airfrog-bin - Airfrog's binary API shared server/client constants and types
//!
//! See [`Binary API`](https://github.com/piersfinlayson/airfrog/blob/main/docs/REST-API.md)
//! for the binary API specification.
//!
//! This crate is `no_std` and platform agnostic.
//!
//! This crate is used by the default airfrog firmware to implement the binary
//! API server.
//!
//! It is used by [`probe-rs`](https://github.com/piersfinlayson/probe-rs) to
//! implement a client for airfrog's binary API.

#![no_std]

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

/// Port used to serve binary API requests.
/// Chosen as AF is 0x4146 in hex.
pub const PORT: u16 = 4146;

// Binary API version
pub const VERSION: u8 = 0x01;

/// Maximum number of words supported on a bulk data request
pub const MAX_WORD_COUNT: u16 = 256;

/// Binary API command types
pub const CMD_DP_READ: u8 = 0x00;
pub const CMD_DP_WRITE: u8 = 0x01;
pub const CMD_AP_READ: u8 = 0x02;
pub const CMD_AP_WRITE: u8 = 0x03;
pub const CMD_AP_BULK_READ: u8 = 0x12;
pub const CMD_AP_BULK_WRITE: u8 = 0x13;
pub const CMD_MULTI_REG_WRITE: u8 = 0x14;
pub const CMD_PING: u8 = 0xF0;
pub const CMD_RESET_TARGET: u8 = 0xF1;
pub const CMD_CLOCK: u8 = 0xF2;
pub const CMD_SET_SPEED: u8 = 0xF3;
pub const CMD_DISCONNECT: u8 = 0xFF;

/// Binary API response codes
pub const RSP_OK: u8 = 0x00;
pub const RSP_ERR_CMD: u8 = 0x81;
pub const RSP_ERR_SWD: u8 = 0x82;
pub const RSP_ERR_TIMEOUT: u8 = 0x83;
pub const RSP_ERR_NET: u8 = 0x84;
pub const RSP_ERR_API: u8 = 0x85;

/// Binary API single byte command codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    DpRead = CMD_DP_READ,
    DpWrite = CMD_DP_WRITE,
    ApRead = CMD_AP_READ,
    ApWrite = CMD_AP_WRITE,
    ApBulkRead = CMD_AP_BULK_READ,
    ApBulkWrite = CMD_AP_BULK_WRITE,
    MultiRegWrite = CMD_MULTI_REG_WRITE,
    Ping = CMD_PING,
    ResetTarget = CMD_RESET_TARGET,
    Clock = CMD_CLOCK,
    SetSpeed = CMD_SET_SPEED,
    Disconnect = CMD_DISCONNECT,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::DpRead => write!(f, "DP Read"),
            Command::DpWrite => write!(f, "DP Write"),
            Command::ApRead => write!(f, "AP Read"),
            Command::ApWrite => write!(f, "AP Write"),
            Command::ApBulkRead => write!(f, "AP Bulk Read"),
            Command::ApBulkWrite => write!(f, "AP Bulk Write"),
            Command::MultiRegWrite => write!(f, "Multi Register Write"),
            Command::Ping => write!(f, "Ping"),
            Command::ResetTarget => write!(f, "Reset Target"),
            Command::Clock => write!(f, "Clock"),
            Command::SetSpeed => write!(f, "Set Speed"),
            Command::Disconnect => write!(f, "Disconnect"),
        }
    }
}

impl Command {
    /// Converts a Command to its byte representation
    ///
    /// Returns:
    /// - `u8`: The byte representation of the command.
    pub fn to_byte(self) -> u8 {
        self as u8
    }

    /// Convert a command byte to a `Command` enum variant
    ///
    /// Arguments:
    /// - `cmd`: The command byte to convert.
    ///
    /// Returns:
    /// - `Ok(Command)`: If the command byte is recognized.
    /// - `Err(ProtocolError::Command)`: If the command byte is not
    ///   recognized.
    pub fn from_byte(cmd: u8) -> Result<Self, ProtocolError> {
        match cmd {
            CMD_DP_READ => Ok(Self::DpRead),
            CMD_DP_WRITE => Ok(Self::DpWrite),
            CMD_AP_READ => Ok(Self::ApRead),
            CMD_AP_WRITE => Ok(Self::ApWrite),
            CMD_AP_BULK_READ => Ok(Self::ApBulkRead),
            CMD_AP_BULK_WRITE => Ok(Self::ApBulkWrite),
            CMD_MULTI_REG_WRITE => Ok(Self::MultiRegWrite),
            CMD_PING => Ok(Self::Ping),
            CMD_RESET_TARGET => Ok(Self::ResetTarget),
            CMD_CLOCK => Ok(Self::Clock),
            CMD_SET_SPEED => Ok(Self::SetSpeed),
            CMD_DISCONNECT => Ok(Self::Disconnect),
            _ => Err(ProtocolError::Command(cmd)),
        }
    }

    /// Determine how many more bytes to read for this command type
    ///
    /// Returns:
    /// - `Ok((usize, bool))`: The number of bytes needed and whether the
    ///   command will receive additional data (i.e. is a bulk command).
    /// - `Err(SwdError::Api)`: If the command is not recognized.
    pub fn remaining_bytes(&self) -> Result<(usize, bool), ProtocolError> {
        match self {
            Self::DpRead => Ok((1, false)),       // reg
            Self::DpWrite => Ok((5, false)),      // reg + data
            Self::ApRead => Ok((1, false)),       // reg
            Self::ApWrite => Ok((5, false)),      // reg + data
            Self::ApBulkRead => Ok((3, true)),    // reg + 2 byte count
            Self::ApBulkWrite => Ok((3, true)),   // reg + 2 byte count (+ N * 4 bytes for data)
            Self::MultiRegWrite => Ok((2, true)), // count
            Self::Ping | Self::ResetTarget | Self::Disconnect => Ok((0, false)), // no additional data
            Self::Clock => Ok((3, false)), // level|post + 2 byte cycles
            Self::SetSpeed => Ok((1, false)), // speed byte
        }
    }

    /// Determines how many variable bytes this command requires
    ///
    /// Arguments:
    /// - `count`: The number of words to read for bulk commands.
    ///
    /// Returns:
    /// - `Ok(usize)`: The number of bytes to read for the command.
    /// - `Err(SwdError::Api)`: If the command is not recognized or if the
    ///   count is invalid.
    pub fn var_bytes(&self, count: u16) -> Result<usize, ProtocolError> {
        match self {
            Self::ApBulkRead | Self::ApBulkWrite => {
                if count > MAX_WORD_COUNT {
                    Err(ProtocolError::Arg)
                } else {
                    Ok(count as usize * 4) // 4 bytes per word
                }
            }
            Self::MultiRegWrite => {
                if count > MAX_WORD_COUNT {
                    Err(ProtocolError::Arg)
                } else {
                    Ok(count as usize * 6) // 1 byte reg type + 1 byte reg + 4 bytes data
                }
            }
            _ => Ok(0), // No variable bytes for other commands
        }
    }
}

/// Binary API single byte response codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ResponseCode {
    Ok = RSP_OK,
    Cmd = RSP_ERR_CMD,
    Swd = RSP_ERR_SWD,
    Timeout = RSP_ERR_TIMEOUT,
    Net = RSP_ERR_NET,
    Api = RSP_ERR_API,
}

impl fmt::Display for ResponseCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResponseCode::Ok => write!(f, "OK"),
            ResponseCode::Cmd => write!(f, "Command Error"),
            ResponseCode::Swd => write!(f, "SWD Error"),
            ResponseCode::Timeout => write!(f, "Timeout Error"),
            ResponseCode::Net => write!(f, "Network Error"),
            ResponseCode::Api => write!(f, "API Error"),
        }
    }
}

/// Airfrog speed
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Speed {
    /// Slow speed, 500 kHz
    Slow = 3,

    /// Medium speed, 1000 kHz
    Medium = 2,

    /// Fast speed, 2000 kHz
    Fast = 1,

    /// Turbo speed, 4000 kHz - this is the default speed
    #[default]
    Turbo = 0,
}

impl Speed {
    /// Convert from kHz value to Speed
    ///
    /// Arguments:
    /// - `khz`: The kHz value to convert.
    ///
    /// Returns:
    /// - `Speed`: The corresponding Speed variant.
    pub fn from_khz(khz: u32) -> Self {
        match khz {
            0..=750 => Speed::Slow,
            751..=1500 => Speed::Medium,
            1501..=3000 => Speed::Fast,
            _ => Speed::Turbo,
        }
    }

    /// Convert from Speed to kHz
    ///
    /// Returns:
    /// - `u32`: The kHz value corresponding to the Speed variant.
    pub fn to_khz(self) -> u32 {
        match self {
            Speed::Slow => 500,
            Speed::Medium => 1000,
            Speed::Fast => 2000,
            Speed::Turbo => 4000,
        }
    }

    /// Convert from a byte value to a Speed
    ///
    /// Arguments:
    /// - `byte`: The byte value to convert.
    ///
    /// Returns:
    /// - `Ok(Speed)`: If the byte value is valid.
    /// - `Err(ProtocolError::Arg)`: If the byte value is not
    ///   recognized.
    pub fn from_byte(byte: u8) -> Result<Self, ProtocolError> {
        match byte {
            0 => Ok(Speed::Turbo),
            1 => Ok(Speed::Fast),
            2 => Ok(Speed::Medium),
            3 => Ok(Speed::Slow),
            _ => {
                debug!("Invalid speed byte: {byte}");
                Err(ProtocolError::Arg)
            }
        }
    }
}

/// Type used to represent errors that can occur in sending or receiving
/// commands over the binary API.
#[derive(Debug)]
pub enum ProtocolError {
    /// Invalid command byte received
    Command(u8),

    /// Invalid argument provided
    Arg,
}

/// Type used to represent errors that can occur in parsing received commands
/// over the binary API.
#[derive(Debug)]
pub enum ParseError<T> {
    Transport(T),
    Protocol(ProtocolError),
}

impl<T> From<ProtocolError> for ParseError<T> {
    fn from(e: ProtocolError) -> Self {
        ParseError::Protocol(e)
    }
}

impl<T> ParseError<T> {
    fn transport(e: T) -> Self {
        ParseError::Transport(e)
    }
}

/// Async reader trait for reading data from a stream
pub trait AsyncReader {
    type Error;
    fn read_exact(&mut self, buf: &mut [u8]) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Sync writer trait for writing data to a stream
pub trait SyncWriter {
    type Error;
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error>;
}

/// Represents the type of register being accessed in a command.
#[derive(Debug, PartialEq, Eq, Clone)]
#[repr(u8)]
pub enum RegType {
    /// Debug Port (DP) register
    Dp = 0x00,
    /// Access Port (AP) register
    Ap = 0x01,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MultiReg {
    pub reg_type: RegType,
    pub reg: u8,
    pub data: u32,
}

/// Represents a binary API operation that can be performed over the Airfrog
/// SWD interface.  See the
/// [Binary API documentation](https://github.com/piersfinlayson/airfrog/blob/main/docs/REST-API.md)
/// for details.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Op {
    DpRead {
        reg: u8,
    },
    DpWrite {
        reg: u8,
        data: u32,
    },
    ApRead {
        reg: u8,
    },
    ApWrite {
        reg: u8,
        data: u32,
    },
    ApBulkRead {
        reg: u8,
        count: u16,
    },
    ApBulkWrite {
        reg: u8,
        data: Vec<u32>,
    },
    MultiRegWrite {
        count: u16,
        data: Vec<MultiReg>,
    },
    Ping,
    ResetTarget,
    Clock {
        level: LineLevel,
        post_level: LineLevel,
        cycles: u16,
    },
    SetSpeed {
        speed: Speed,
    },
    Disconnect,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            match self {
                Op::DpRead { reg } => write!(f, "DpRead(reg={reg})"),
                Op::DpWrite { reg, data } => write!(f, "DpWrite(reg={reg}, data={data})"),
                Op::ApRead { reg } => write!(f, "ApRead(reg={reg})"),
                Op::ApWrite { reg, data } => write!(f, "ApWrite(reg={reg}, data={data})"),
                Op::ApBulkRead { reg, count } => {
                    write!(f, "ApBulkRead(reg={reg}, count={count})")
                }
                Op::ApBulkWrite { reg, data } => {
                    write!(f, "ApBulkWrite(reg={reg}, data={data:?})")
                }
                Op::MultiRegWrite { count, data } => {
                    write!(f, "MultiRegWrite(count={count}, data={data:?})")
                }
                Op::Ping => write!(f, "Ping"),
                Op::ResetTarget => write!(f, "ResetTarget"),
                Op::Clock {
                    level,
                    post_level,
                    cycles,
                } => {
                    write!(
                        f,
                        "Clock(level={level:?}, post_level={post_level:?}, cycles={cycles})",
                    )
                }
                Op::SetSpeed { speed } => write!(f, "SetSpeed(speed={speed:?})"),
                Op::Disconnect => write!(f, "Disconnect"),
            }
        } else {
            match self {
                Op::DpRead { .. } => write!(f, "DP Read"),
                Op::DpWrite { .. } => write!(f, "DP Write"),
                Op::ApRead { .. } => write!(f, "AP Read"),
                Op::ApWrite { .. } => write!(f, "AP Write"),
                Op::ApBulkRead { .. } => write!(f, "AP Bulk Read"),
                Op::ApBulkWrite { .. } => write!(f, "AP Bulk Write"),
                Op::MultiRegWrite { .. } => write!(f, "Multi Register Write"),
                Op::Ping => write!(f, "Ping"),
                Op::ResetTarget => write!(f, "Reset Target"),
                Op::Clock { .. } => write!(f, "Clock"),
                Op::SetSpeed { .. } => write!(f, "Set Speed"),
                Op::Disconnect => write!(f, "Disconnect"),
            }
        }
    }
}

// Public Op methods
impl Op {
    pub async fn recv_cmd<R: AsyncReader>(reader: &mut R) -> Result<Command, ParseError<R::Error>> {
        let mut cmd = [0u8; 1];
        reader
            .read_exact(&mut cmd)
            .await
            .map_err(ParseError::transport)?;
        let command = Command::from_byte(cmd[0])?;
        Ok(command)
    }

    /// Used by an airfrog binary API server to receive a complete command from
    /// an API client.
    ///
    /// Called once the command byte has been read from the stream.
    ///
    /// Arguments:
    /// - `cmd`: The command byte received.
    /// - `reader`: A mutable reference to a reader that implements the
    ///   `Reader` trait
    ///
    /// Returns:
    /// - `Ok(Self)`: If the command was successfully parsed.
    /// - `Err(ParseError<R::Error>)`: If there was an error parsing the
    ///   received command.
    pub async fn async_recv<R: AsyncReader>(
        command: Command,
        reader: &mut R,
    ) -> Result<Self, ParseError<R::Error>> {
        trace!("Received command: {command}");
        let (bytes_needed, _) = command.remaining_bytes()?;

        let buf = if bytes_needed > 0 {
            let mut buf = vec![0u8; bytes_needed];
            reader
                .read_exact(&mut buf)
                .await
                .inspect_err(|_| debug!("Failed to read static command bytes {bytes_needed}"))
                .map_err(ParseError::transport)?;
            buf
        } else {
            vec![]
        };

        match command {
            Command::DpRead => Ok(Op::DpRead { reg: buf[0] }),
            Command::DpWrite => {
                let reg = buf[0];
                let data = Self::parse_word(&buf[1..5])?;
                Ok(Op::DpWrite { reg, data })
            }
            Command::ApRead => Ok(Op::ApRead { reg: buf[0] }),
            Command::ApWrite => {
                let reg = buf[0];
                let data = Self::parse_word(&buf[1..5])?;
                Ok(Op::ApWrite { reg, data })
            }
            Command::ApBulkRead => {
                let reg = buf[0];
                let count = Self::parse_count(&buf[1..3])?;
                Ok(Op::ApBulkRead { reg, count })
            }
            Command::ApBulkWrite => {
                let reg = buf[0];
                let count = Self::parse_count(&buf[1..3])?;

                // Read in the additional data bytes
                let data_bytes = command.var_bytes(count)?;
                let mut data_buf = vec![0u8; data_bytes];
                reader
                    .read_exact(&mut data_buf)
                    .await
                    .inspect_err(|_| debug!("Failed to read variable command bytes {data_bytes}"))
                    .map_err(ParseError::transport)?;

                // Process them
                let mut data = Vec::with_capacity(count as usize);
                for chunk in data_buf.chunks_exact(4) {
                    data.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }

                Ok(Op::ApBulkWrite { reg, data })
            }
            Command::MultiRegWrite => {
                let count = Self::parse_count(&buf[0..2])?;

                // Read in the additional data bytes
                let data_bytes = command.var_bytes(count)?;
                let mut data_buf = vec![0u8; data_bytes];
                reader
                    .read_exact(&mut data_buf)
                    .await
                    .inspect_err(|_| debug!("Failed to read variable command bytes {data_bytes}"))
                    .map_err(ParseError::transport)?;

                // Process them
                let mut data = Vec::with_capacity(count as usize);
                for chunk in data_buf.chunks_exact(6) {
                    let reg_type = match chunk[0] {
                        0x00 => RegType::Dp,
                        0x01 => RegType::Ap,
                        _ => return Err(ParseError::Protocol(ProtocolError::Arg)),
                    };
                    let reg = chunk[1];
                    let value = u32::from_le_bytes([chunk[2], chunk[3], chunk[4], chunk[5]]);
                    data.push(MultiReg {
                        reg_type,
                        reg,
                        data: value,
                    });
                }
                Ok(Op::MultiRegWrite { count, data })
            }
            Command::Ping => Ok(Op::Ping),
            Command::ResetTarget => Ok(Op::ResetTarget),
            Command::Clock => {
                let (level, post_level) = LineLevel::levels_from_byte(buf[0])?;
                let cycles = u16::from_le_bytes([buf[1], buf[2]]);
                Ok(Op::Clock {
                    level,
                    post_level,
                    cycles,
                })
            }
            Command::SetSpeed => {
                let speed = Speed::from_byte(buf[0])?;
                Ok(Op::SetSpeed { speed })
            }
            Command::Disconnect => Ok(Op::Disconnect),
        }
    }

    /// Used by an airfrog binary API client to send a command to an API
    /// server.
    ///
    /// Arguments:
    /// - `writer`: A mutable reference to a writer that implements the
    ///   `Writer` trait
    ///
    /// Returns:
    /// - `Ok(())`: If the command was successfully sent.
    /// - `Err(W::Error)`: If there was an error sending the command.
    pub fn sync_send<W: SyncWriter>(&self, _writer: &mut W) -> Result<(), W::Error> {
        todo!()
    }
}

// Internal Op methods
impl Op {
    fn parse_word(bytes: &[u8]) -> Result<u32, ProtocolError> {
        if bytes.len() != 4 {
            debug!("Invalid word bytes: {bytes:?}");
            return Err(ProtocolError::Arg);
        }
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn parse_count(bytes: &[u8]) -> Result<u16, ProtocolError> {
        if bytes.len() != 2 {
            debug!("Invalid count bytes: {bytes:?}");
            return Err(ProtocolError::Arg);
        }
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
}

/// Used to represent the SWDIO line state on clock operations.
#[derive(Debug, PartialEq, Eq, Clone)]
#[repr(u8)]
pub enum LineLevel {
    Low = 0,
    High = 1,
    Input = 2,
}

impl LineLevel {
    /// Convert from a byte value to a LineLevel
    ///
    /// Arguments:
    /// - `value`: The byte value to convert.
    ///
    /// Returns:
    /// - `Ok((LineLevel, LineLevel)`: the leve and post level.
    /// - `Err(ProtocolError::Arg)`: If the byte value is not
    ///   recognized.
    pub fn levels_from_byte(value: u8) -> Result<(Self, Self), ProtocolError> {
        let level_bits = value & 0x0F;
        let level = match level_bits {
            0 => LineLevel::Low,
            1 => LineLevel::High,
            2 => LineLevel::Input,
            _ => {
                debug!("Invalid level bits: {level_bits}");
                return Err(ProtocolError::Arg);
            }
        };

        let post_bits = value >> 4;
        let post_level = match post_bits {
            0 => LineLevel::Low,
            1 => LineLevel::High,
            2 => LineLevel::Input,
            _ => {
                debug!("Invalid post level bits: {post_bits}");
                return Err(ProtocolError::Arg);
            }
        };

        Ok((level, post_level))
    }
}
