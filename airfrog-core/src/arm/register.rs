// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! ARM SWD Register Access Traits
//!
//! These are used to ensure strongly typed access to reading and writing SWD
//! registers, using
//!
//! * `airfrog_swd::interface::SwdInterface::read_dp_register`
//! * `airfrog_swd::interface::SwdInterface::read_ap_register`
//! * `airfrog_swd::interface::SwdInterface::write_dp_register`
//! * `airfrog_swd::interface::SwdInterface::write_ap_register`
//!
//! Unless you are extending the debug/SWD protocol support, it is unlikely
//! that you will need to use these traits directly.

/// Base trait for all ARM debug register descriptors
pub trait RegisterDescriptor {
    const ADDRESS: u8;
    type Value;
}

/// Registers that can be read
pub trait ReadableRegister: RegisterDescriptor {
    /// Convert raw 32-bit data to register value
    fn from_raw(data: u32) -> Self::Value
    where
        Self::Value: From<u32>,
    {
        Self::Value::from(data)
    }
}

/// Registers that can be written  
pub trait WritableRegister: RegisterDescriptor {
    /// Convert register value to raw 32-bit data
    fn to_raw(value: Self::Value) -> u32
    where
        Self::Value: Into<u32>,
    {
        value.into()
    }
}

/// Debug Port registers (accessed via DP operations)
pub trait DpRegister: RegisterDescriptor {}

/// Access Port registers (accessed via AP operations)
pub trait ApRegister: RegisterDescriptor {}

/// Generate a read-only register data type
#[macro_export]
macro_rules! register_data_r {
    ($name:ident) => {
        // Used to retrieve value
        impl From<$name> for u32 {
            fn from(value: $name) -> u32 {
                value.0
            }
        }

        impl From<u32> for $name {
            fn from(value: u32) -> Self {
                $name(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "0x{:08X}", self.0)
            }
        }
    };
}

/// Generate a read-write register data type
#[macro_export]
macro_rules! register_data_rw {
    ($name:ident) => {
        impl From<$name> for u32 {
            fn from(value: $name) -> u32 {
                value.0
            }
        }

        impl From<u32> for $name {
            fn from(value: u32) -> Self {
                $name(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "0x{:08X}", self.0)
            }
        }
    };
}

/// Generate a write-only register data type  
#[macro_export]
macro_rules! register_data_w {
    ($name:ident) => {
        impl From<$name> for u32 {
            fn from(value: $name) -> u32 {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "0x{:08X}", self.0)
            }
        }
    };
}
