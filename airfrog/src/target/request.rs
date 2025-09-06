// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Target Request and related types

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use airfrog_swd::protocol::{LineState, Speed};

use crate::http::{Method, Rest, json::parse_json_body};
use crate::target::{Response, Settings, SettingsSource};
use crate::{AirfrogError, ErrorKind};

/// A request sent to the Target to get it to perform an SWD or related
/// operation
pub struct Request {
    pub command: Command,
    pub response_signal: &'static Signal<CriticalSectionRawMutex, Response>,
}

// Different IPC commands that Httpd can sent to Target as part of a Request
#[derive(Debug)]
pub enum Command {
    // REST

    // Target Control
    GetStatus,
    Reset,
    GetDetails,
    ClearErrors,
    GetErrors,

    // Memory Operations
    ReadMem {
        addr: String,
    },
    WriteMem {
        addr: String,
        data: String,
    },
    ReadMemBulk {
        addr: String,
        count: usize,
    },
    WriteMemBulk {
        addr: String,
        data: Vec<String>,
    },

    // Flash Operations
    UnlockFlash,
    LockFlash,
    EraseSector {
        sector: u32,
    },
    EraseAll,
    WriteFlashWord {
        addr: String,
        data: String,
    },
    WriteFlashBulk {
        addr: String,
        data: Vec<String>,
    },

    // Config Operations
    GetSpeed,
    SetSpeed {
        speed: Speed,
    },
    UpdateSettings {
        source: SettingsSource,
        settings: Settings,
    },

    // Raw Register Operations
    RawReset,
    RawReadDpReg {
        register: String,
    },
    RawWriteDpReg {
        register: String,
        data: String,
    },
    RawReadApReg {
        ap_index: String,
        register: String,
    },
    RawWriteApReg {
        ap_index: String,
        register: String,
        data: String,
    },
    RawBulkReadApReg {
        ap_index: String,
        register: String,
        count: usize,
    },
    RawBulkWriteApReg {
        ap_index: String,
        register: String,
        count: usize,
        data: Vec<String>,
    },
    Clock {
        level: LineState,
        post_level: LineState,
        count: u32,
    },
}

impl Command {
    pub fn from_rest(
        rest_type: Rest,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> Result<Self, AirfrogError> {
        // Parse JSON if present
        let body = parse_json_body(body)?;

        match rest_type {
            Rest::Target => Self::from_rest_target(method, path, body),
            Rest::Raw => Self::from_rest_raw(method, path, body),
            Rest::SwdConfig => Self::from_rest_config(method, path, body),
        }
    }

    // /api/config/swd
    fn from_rest_config(
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Self, AirfrogError> {
        match (method, path) {
            (Method::Get, "/runtime/speed") => Ok(Command::GetSpeed),
            (Method::Post, "/runtime/speed") => {
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let speed_str = body["speed"]
                    .as_str()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let speed = match speed_str {
                    "Slow" => Speed::Slow,
                    "Medium" => Speed::Medium,
                    "Fast" => Speed::Fast,
                    "Turbo" => Speed::Turbo,
                    _ => return Err(AirfrogError::Airfrog(ErrorKind::InvalidBody)),
                };
                Ok(Command::SetSpeed { speed })
            }
            (Method::Post, "/runtime") => {
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let settings = serde_json::from_value(body)
                    .map_err(|_| AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                Ok(Command::UpdateSettings {
                    source: SettingsSource::Runtime,
                    settings,
                })
            }
            (Method::Post, "/swd/flash") => {
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let settings = serde_json::from_value(body)
                    .map_err(|_| AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                Ok(Command::UpdateSettings {
                    source: SettingsSource::Flash,
                    settings,
                })
            }
            _ => Err(AirfrogError::Airfrog(ErrorKind::InvalidPath)),
        }
    }

    // /api/target/status
    fn from_rest_target(
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Self, AirfrogError> {
        match (method, path) {
            // Target Control
            (Method::Get, "/status") => Ok(Command::GetStatus),
            (Method::Post, "/reset") => Ok(Command::Reset),
            (Method::Get, "/details") => Ok(Command::GetDetails),
            (Method::Get, "/errors") => Ok(Command::GetErrors),
            (Method::Post, "/clear-errors") => Ok(Command::ClearErrors),

            // Memory Operations
            (Method::Get, path) if path.starts_with("/memory/read/") => {
                let addr = path
                    .strip_prefix("/memory/read/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?
                    .to_string();
                Ok(Command::ReadMem { addr })
            }
            (Method::Post, path) if path.starts_with("/memory/write/") => {
                let addr = path
                    .strip_prefix("/memory/write/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?
                    .to_string();
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data = body["data"]
                    .as_str()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    .to_string();
                Ok(Command::WriteMem { addr, data })
            }
            (Method::Post, path) if path.starts_with("/memory/bulk/read/") => {
                let addr = path
                    .strip_prefix("/memory/bulk/read/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?
                    .to_string();
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let count = body["count"]
                    .as_u64()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    as usize;
                Ok(Command::ReadMemBulk { addr, count })
            }
            (Method::Post, path) if path.starts_with("/memory/bulk/write/") => {
                let addr = path
                    .strip_prefix("/memory/bulk/write/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?
                    .to_string();
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data_array = body["data"]
                    .as_array()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data: Vec<String> = data_array
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))
                            .map(String::from)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Command::WriteMemBulk { addr, data })
            }

            // Flash Operations
            (Method::Post, "/flash/unlock") => Ok(Command::UnlockFlash),
            (Method::Post, "/flash/lock") => Ok(Command::LockFlash),
            (Method::Post, path) if path.starts_with("/flash/erase-sector/") => {
                let addr = path
                    .strip_prefix("/flash/erase-sector/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?;
                let sector = addr
                    .parse::<u32>()
                    .map_err(|_| AirfrogError::Airfrog(ErrorKind::InvalidPath))?;
                Ok(Command::EraseSector { sector })
            }
            (Method::Post, "/flash/erase-all") => Ok(Command::EraseAll),
            (Method::Post, path) if path.starts_with("/flash/write/") => {
                let addr = path
                    .strip_prefix("/flash/write/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?
                    .to_string();
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data = body["data"]
                    .as_str()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    .to_string();
                Ok(Command::WriteFlashWord { addr, data })
            }
            (Method::Post, path) if path.starts_with("/flash/bulk/write/") => {
                let addr = path
                    .strip_prefix("/flash/bulk/write/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?
                    .to_string();
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data_array = body["data"]
                    .as_array()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data: Vec<String> = data_array
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))
                            .map(String::from)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Command::WriteFlashBulk { addr, data })
            }

            _ => Err(AirfrogError::Airfrog(ErrorKind::InvalidPath)),
        }
    }

    // /api/raw
    fn from_rest_raw(
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Self, AirfrogError> {
        match (method, path) {
            // Raw Register Operations
            (Method::Post, "/reset") => Ok(Command::RawReset),
            (Method::Get, path) if path.starts_with("/dp/read/") => {
                let register = path
                    .strip_prefix("/dp/read/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?
                    .to_string();
                Ok(Command::RawReadDpReg { register })
            }
            (Method::Post, path) if path.starts_with("/dp/write/") => {
                let register = path
                    .strip_prefix("/dp/write/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?
                    .to_string();
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data = body["data"]
                    .as_str()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    .to_string();
                Ok(Command::RawWriteDpReg { register, data })
            }
            (Method::Get, path) if path.starts_with("/ap/read/") => {
                let path_params = path
                    .strip_prefix("/ap/read/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?;
                let parts: Vec<&str> = path_params.split('/').collect();
                if parts.len() != 2 {
                    return Err(AirfrogError::Airfrog(ErrorKind::InvalidPath));
                }
                let ap_index = parts[0].to_string();
                let register = parts[1].to_string();
                Ok(Command::RawReadApReg { ap_index, register })
            }
            (Method::Post, path) if path.starts_with("/ap/write/") => {
                let path_params = path
                    .strip_prefix("/ap/write/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?;
                let parts: Vec<&str> = path_params.split('/').collect();
                if parts.len() != 2 {
                    return Err(AirfrogError::Airfrog(ErrorKind::InvalidPath));
                }
                let ap_index = parts[0].to_string();
                let register = parts[1].to_string();
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data = body["data"]
                    .as_str()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    .to_string();
                Ok(Command::RawWriteApReg {
                    ap_index,
                    register,
                    data,
                })
            }
            (Method::Post, path) if path.starts_with("/ap/bulk/read/") => {
                let path_params = path
                    .strip_prefix("/ap/bulk/read/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?;
                let parts: Vec<&str> = path_params.split('/').collect();
                if parts.len() != 2 {
                    return Err(AirfrogError::Airfrog(ErrorKind::InvalidPath));
                }
                let ap_index = parts[0].to_string();
                let register = parts[1].to_string();
                let count = body
                    .and_then(|b| b["count"].as_u64())
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    as usize;
                Ok(Command::RawBulkReadApReg {
                    ap_index,
                    register,
                    count,
                })
            }
            (Method::Post, path) if path.starts_with("/ap/bulk/write/") => {
                let path_params = path
                    .strip_prefix("/ap/bulk/write/")
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidPath))?;
                let parts: Vec<&str> = path_params.split('/').collect();
                if parts.len() != 2 {
                    return Err(AirfrogError::Airfrog(ErrorKind::InvalidPath));
                }
                let ap_index = parts[0].to_string();
                let register = parts[1].to_string();
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let count = body["count"]
                    .as_u64()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    as usize;
                let data_array = body["data"]
                    .as_array()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let data: Vec<String> = data_array
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))
                            .map(String::from)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Command::RawBulkWriteApReg {
                    ap_index,
                    register,
                    count,
                    data,
                })
            }
            (Method::Post, "/clock") => {
                let body = body.ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?;
                let level = body["level"]
                    .as_str()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    .to_string();
                let level = Self::get_level(&level)?;
                let post_level = body["post_level"]
                    .as_str()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    .to_string();
                let post_level = Self::get_level(&post_level)?;
                let count = body["count"]
                    .as_u64()
                    .ok_or(AirfrogError::Airfrog(ErrorKind::InvalidBody))?
                    as u32;
                Ok(Command::Clock {
                    level,
                    post_level,
                    count,
                })
            }

            _ => Err(AirfrogError::Airfrog(ErrorKind::InvalidPath)),
        }
    }

    fn get_level(level_str: &str) -> Result<LineState, AirfrogError> {
        let level_str = level_str.to_lowercase();
        if level_str == "low" {
            Ok(LineState::Low)
        } else if level_str == "high" {
            Ok(LineState::High)
        } else if level_str == "input" {
            Ok(LineState::Input)
        } else {
            // Invalid clock level
            debug!("Invalid clock level: {level_str}");
            Err(AirfrogError::Airfrog(ErrorKind::InvalidBody))
        }
    }
}
