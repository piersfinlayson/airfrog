// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog - Macros

// Macro to handle (binary) API timeouts
#[macro_export]
macro_rules! with_timeout {
    ($future:expr) => {
        match embassy_time::with_timeout(BINARY_API_TIMEOUT, $future).await {
            Ok(result) => result,
            Err(_) => {
                warn!("Timeout occurred");
                return Err(AirfrogError::Airfrog(ErrorKind::Timeout));
            }
        }
    };
}
