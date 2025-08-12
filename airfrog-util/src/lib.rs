// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! Airfrog is the tiny wireless co-processor for ARM.
//!
//! <https://piers.rocks/u/airfrog>
//!
//! airfrog-util - General development utilities and helpers for building
//! Airfrog firmware.
//!
//! [`net`] - provides a helper for WiFi and networking, using `esp-wifi` and
//! `embassy-net`.

#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

pub mod net;
