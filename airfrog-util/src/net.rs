// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

//! airfrog-util - Networking utilities and helpers
//!
//! The [`Wifi`] object provides a way to configure and control Airfrog's WiFi
//! interfaces.
//!  
//! # Example
//! ```rust
//! use airfrog_util::net::{Wifi, InterfaceConfig, WifiType, Control};
//! use embassy_net::StackResources;
//!
//! // Create a STA interface config, and the static resources `embassy-net`
//! // requires to run the networking stack.
//! let sta_stack_resources = make_static!(StackResources::<2>::new());
//! let sta_config = InterfaceConfig {
//!     ssid: String::from("MyNetwork"),
//!     password: String::from("password123"),
//!     net: embassy_net::Config::dhcpv4(Default::default()),
//! };
//!
//! // Create the WiFi object using the builder pattern.  Builds all required
//! // `esp-wifi` and `embassy-net` objects.
//! // <2, 0> are the number of sockets to use for the STA and AP interfaces
//! let mut wifi = Wifi::builder::<2, 0>()
//!     .with_sta_if(sta_config, sta_stack_resources)
//!     .build(&spawner, timg0, rng, wifi_hw)
//!     .expect("Failed to build WiFi object");
//!
//! // Spawn the WiFi and networking tasks.
//! wifi.must_spawn();
//!
//! // Start the STA interface.
//! wifi.control_and_wait(WifiType::Sta, WifiControl::Enable).await;
//!
//! // Use other `Wifi` objects to get the networking stack, to create sockets,
//! // wait for an IP address, etc.
//! ```

use alloc::format;
use alloc::string::String;
use core::fmt;
use core::future::pending;
use embassy_executor::Spawner;
use embassy_futures::select::{Either3, select3};
use embassy_net::{Config as NetConfig, Runner, Stack, StackResources, StaticConfigV4};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use esp_hal::peripherals::{RNG, TIMG0, WIFI};
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::wifi::{
    AccessPointConfiguration, ClientConfiguration, Configuration, WifiController, WifiDevice,
    WifiEvent, WifiMode,
};
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use static_cell::make_static;

const AP_CHANNEL: u8 = 1;
const MAX_AP_CONNECTIONS: u16 = 2;

/// Error type for WiFi operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Hit error in the esp-wifi stack
    Wifi(String),

    /// Configuration error, e.g. missing required configuration
    Config(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Wifi(msg) => write!(f, "WiFi stack error: {msg}"),
            Error::Config(msg) => write!(f, "Configuration error: {msg}"),
        }
    }
}

/// WiFi controls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Enable the WiFi interface
    Enable,

    /// Disable the WiFi interface
    Disable,
}

/// WiFi interface status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// WiFi interface is enabled
    Enabled,

    /// WiFi interface is disabled
    Disabled,

    /// WiFi interface is connected
    Connected,

    /// WiFi interface is disconnected
    Disconnected,
}

/// Type of WiFi interface
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiType {
    Sta,
    Ap,
}

// Signals used to command the Wifi controller, and provide notifications about
// changes in WiFi state.  Used internally with pub [`Wifi`] using them.
static CONTROL_STA: Signal<CriticalSectionRawMutex, Control> = Signal::new();
static CONTROL_AP: Signal<CriticalSectionRawMutex, Control> = Signal::new();
static STATUS_STA: Signal<CriticalSectionRawMutex, Status> = Signal::new();
static STATUS_AP: Signal<CriticalSectionRawMutex, Status> = Signal::new();

/// Configuration for a WiFi interface.  For an AP, ensure the password is at
/// least 8 characters long, otherwise esp-wifi will return an error.
// Do not derive Debug as there appears to be a bug in the embassy-net crate
// leading to a crash when trying to print the Debug representation of
// (Net)Config
#[derive(Clone)]
pub struct InterfaceConfig {
    /// SSID of the WiFi network
    pub ssid: String,

    /// Password for the WiFi network
    pub password: String,

    /// Network configuration for the WiFi interface.  Either a static IP or
    /// DHCP configuration.  It is likely you want to use static IP for an AP
    /// interface.
    pub net: NetConfig,
}

impl core::fmt::Debug for InterfaceConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // DO NOT output net
        f.debug_struct("InterfaceConfig")
            .field("ssid", &self.ssid)
            .field("password", &self.password)
            .finish()
    }
}

/// Builder for the WiFi interface.  Use [`Wifi::builder`] to create a new
/// instance of this builder and see the documentation for that method for
/// examples of how to use it.
#[derive(Default)]
pub struct WifiBuilder<const STA: usize, const AP: usize> {
    sta_config: Option<InterfaceConfig>,
    ap_config: Option<InterfaceConfig>,
    sta_stack_resources: Option<&'static mut StackResources<STA>>,
    ap_stack_resources: Option<&'static mut StackResources<AP>>,
}

impl<const STA: usize, const AP: usize> WifiBuilder<STA, AP> {
    fn new() -> Self {
        Self::default()
    }

    /// Adds a STA (station) interface configuration to the builder.
    ///
    /// Arguments:
    /// - `config`: The configuration for the STA interface, including SSID,
    ///   password, and network configuration.
    /// - `stack_resources`: The stack resources for the STA interface, which
    ///   are used to manage the networking stack.
    ///
    /// Returns:
    /// - `Self` to allow method chaining.
    pub fn with_sta_if(
        mut self,
        config: InterfaceConfig,
        stack_resources: &'static mut StackResources<STA>,
    ) -> Self {
        self.sta_config = Some(config);
        self.sta_stack_resources = Some(stack_resources);
        self
    }

    /// Adds an AP (access point) interface configuration to the builder.
    ///
    /// Arguments:
    /// - `config`: The configuration for the AP interface, including SSID,
    ///   password, and network configuration.
    /// - `stack_resources`: The stack resources for the AP interface, which
    ///   are used to manage the networking stack.
    ///
    /// Returns:
    /// - `Self` to allow method chaining.
    pub fn with_ap_if(
        mut self,
        config: InterfaceConfig,
        stack_resources: &'static mut StackResources<AP>,
    ) -> Self {
        self.ap_config = Some(config);
        self.ap_stack_resources = Some(stack_resources);
        self
    }

    /// Builds the WiFi interface with the specified configurations.
    ///
    /// After this function you likely want to call [`Wifi::must_spawn`] to
    /// start the various networking and WiFi tasks.
    ///
    /// Arguments:
    /// - `spawner`: The spawner used to spawn the WiFi tasks.
    /// - `timg0``: The timer group used for WiFi timing.
    /// - `rng`: The random number generator used for WiFi operations.
    /// - `wifi`: The WiFi peripheral to use for the WiFi interface.
    ///
    /// Returns:
    /// - `Ok(Wifi)` if the WiFi interface was built successfully.
    /// - `Err(Error)` if there was an error building the WiFi interface.
    pub fn build(
        self,
        spawner: &Spawner,
        timg0: TIMG0<'static>,
        rng: RNG<'static>,
        wifi: WIFI<'static>,
    ) -> Result<Wifi, Error> {
        // Now initialize your Wifi object with these resources
        let mut wifi_obj = Wifi::new(spawner);
        wifi_obj.init(
            timg0,
            rng,
            wifi,
            self.sta_config,
            self.ap_config,
            self.sta_stack_resources,
            self.ap_stack_resources,
        )?;
        Ok(wifi_obj)
    }
}

/// Main WiFi object, used to add WiFi capability to an Airfrog application.
///
/// Uses `esp-wifi` and `embassy-net`.
///
/// See [`Wifi::builder`] for an example of creating and starting WiFi using
/// this object.
pub struct Wifi {
    spawner: Spawner,
    controller: Option<WifiController<'static>>,
    sta_stack: Option<Stack<'static>>,
    ap_stack: Option<Stack<'static>>,
    sta_runner: Option<Runner<'static, WifiDevice<'static>>>,
    ap_runner: Option<Runner<'static, WifiDevice<'static>>>,
}

impl Wifi {
    /// Creates a new WiFi builder with the specified resource (socket) sizes
    /// for STA and AP interfaces.
    ///
    /// Generics:
    /// - `STA`: The number of sockets for the STA interface
    /// - `AP`: The number of sockets for the AP interface
    ///
    /// Returns:
    /// - `WifiBuilder<STA, AP>` where STA is the number of sockets for the STA
    ///   interface and AP is the number of sockets for the AP interface.
    pub fn builder<const STA: usize, const AP: usize>() -> WifiBuilder<STA, AP> {
        WifiBuilder::new()
    }

    // Creates a new WiFi instance with the specified configuration.
    fn new(spawner: &Spawner) -> Self {
        Self {
            spawner: *spawner,
            controller: None,
            sta_stack: None,
            ap_stack: None,
            sta_runner: None,
            ap_runner: None,
        }
    }

    // Initializes the WiFi controller and creates the WiFi interfaces (AP
    // and STA).
    //
    // Arguments:
    // - `timg0`: The timer group used for WiFi timing
    // - `rng`: The random number generator used for WiFi operations
    // - `wifi`: The WiFi peripheral
    // - `sta_stack_resources`: Optional stack resources for the STA interface
    // - `ap_stack_resources`: Optional stack resources for the AP interface
    //
    // Returns:
    // - `Ok(())` if the WiFi controller was initialized successfully
    #[allow(clippy::too_many_arguments)]
    fn init<const STA: usize, const AP: usize>(
        &mut self,
        timg0: TIMG0<'static>,
        rng: RNG<'static>,
        wifi: WIFI<'static>,
        sta_config: Option<InterfaceConfig>,
        ap_config: Option<InterfaceConfig>,
        sta_stack_resources: Option<&'static mut StackResources<STA>>,
        ap_stack_resources: Option<&'static mut StackResources<AP>>,
    ) -> Result<(), Error> {
        // Set up the peripherals for WiFi
        let timg0 = TimerGroup::new(timg0);
        let mut rng = Rng::new(rng);

        // Create and configure the WiFi controller.
        // Use &* to make the mutable reference that make_static! returns
        // immutable, which is what esp_wifi expects.
        let esp_wifi_ctrl = &*make_static!(esp_wifi::init(timg0.timer0, rng).unwrap());
        let (mut controller, interfaces) = esp_wifi::wifi::new(esp_wifi_ctrl, wifi).unwrap();

        // Configure and store the controller
        self.configure_wifi(&mut controller, sta_config.as_ref(), ap_config.as_ref())?;
        self.controller = Some(controller);

        // Set up the the STA interface, if configured.
        if let Some(sta_config) = sta_config {
            debug!(
                "Info:  Configuring STA interface with SSID: {}",
                sta_config.ssid
            );
            let sta_seed = (rng.random() as u64) << 32 | rng.random() as u64;
            let (sta_stack, sta_runner) = embassy_net::new(
                interfaces.sta,
                sta_config.net.clone(),
                sta_stack_resources.expect("STA stack resources not provided"),
                sta_seed,
            );
            self.sta_stack = Some(sta_stack);
            self.sta_runner = Some(sta_runner);
        }

        // Set up the AP interface, if configured.
        if let Some(ap_config) = ap_config {
            debug!(
                "Info:  Configuring AP interface with SSID: {}",
                ap_config.ssid
            );
            let ap_seed = (rng.random() as u64) << 32 | rng.random() as u64;
            let (ap_stack, ap_runner) = embassy_net::new(
                interfaces.ap,
                ap_config.net.clone(),
                ap_stack_resources.expect("AP stack resources not provided"),
                ap_seed,
            );
            self.ap_stack = Some(ap_stack);
            self.ap_runner = Some(ap_runner);
        }

        Ok(())
    }

    // Configures the WiFi controller
    fn configure_wifi(
        &self,
        controller: &mut WifiController<'static>,
        sta_if: Option<&InterfaceConfig>,
        ap_if: Option<&InterfaceConfig>,
    ) -> Result<(), Error> {
        // Avoid power saving mode for more reliable WiFi
        controller
            .set_power_saving(esp_wifi::config::PowerSaveMode::None)
            .inspect_err(|e| {
                error!("Error: Failed to set power WiFi saving mode {e:?}");
            })
            .ok();

        // Create the STA and AP configuration
        let sta_config = if let Some(sta_if) = sta_if {
            debug!(
                "Info:  Configuring STA interface with SSID: {}",
                sta_if.ssid
            );
            debug!("Info:  STA interface password: {}", sta_if.password);
            Some(ClientConfiguration {
                ssid: sta_if.ssid.clone(),
                password: sta_if.password.clone(),
                ..Default::default()
            })
        } else {
            debug!("Info:  No STA interface configured");
            None
        };
        let ap_config = if let Some(ap_if) = ap_if {
            debug!("Info:  Configuring AP interface with SSID: {}", ap_if.ssid);
            debug!("Info:  AP interface password: {}", ap_if.password);
            Some(AccessPointConfiguration {
                ssid: ap_if.ssid.clone(),
                password: ap_if.password.clone(),
                channel: AP_CHANNEL,
                max_connections: MAX_AP_CONNECTIONS,
                auth_method: esp_wifi::wifi::AuthMethod::WPA2Personal,
                ssid_hidden: false,
                secondary_channel: None,
                ..Default::default()
            })
        } else {
            debug!("Info:  No AP interface configured");
            None
        };

        let config = match (sta_config, ap_config) {
            (Some(sta), Some(ap)) => Configuration::Mixed(sta, ap),
            (Some(sta), None) => Configuration::Client(sta),
            (None, Some(ap)) => Configuration::AccessPoint(ap),
            (None, None) => return Ok(()), // No config
        };

        controller
            .set_configuration(&config)
            .inspect(|_| trace!("Ok:    WiFi configuration set successfully"))
            .inspect_err(|e| {
                warn!("Error: Failed to set WiFi configuration: {e:?}");
            })
            .map_err(|e| Error::Wifi(format!("Failed to set WiFi configuration: {e:?}")))
    }

    /// Spawns the WiFi and networking tasks.  Networking tasks are spawned
    /// first, so they are ready to handle events when the WiFi connection is
    /// established.
    ///
    /// Uses `Spawner::must_spawn` to ensure that the tasks are spawned or
    /// panics.
    pub fn must_spawn(&mut self) {
        // Start the STA runner
        if self.sta_runner.is_some() {
            let sta_runner = self.sta_runner.take().unwrap();
            self.spawner.must_spawn(net_task(sta_runner));
        }

        // Start the AP runner
        if self.ap_runner.is_some() {
            let ap_runner = self.ap_runner.take().unwrap();
            self.spawner.must_spawn(net_task(ap_runner));
        }

        // Start the WiFi controller task
        let controller = self
            .controller
            .take()
            .expect("WiFi controller not initialized");
        self.spawner.must_spawn(wifi_controller(controller));
    }

    /// Waits for a control update for the specified WiFi type.  This is
    /// typically received after a control signal is sent to enable or disable
    /// the WiFi interface, using [`Self::control`].
    ///
    /// Arguments:
    /// - `wifi_type`: The type of WiFi interface to wait for (STA or AP)
    ///
    /// Returns:
    /// - `Status` indicating the current status of the WiFi interface
    pub async fn wait_for_control_update(&self, wifi_type: WifiType) -> Status {
        match wifi_type {
            WifiType::Sta => STATUS_STA.wait().await,
            WifiType::Ap => STATUS_AP.wait().await,
        }
    }

    /// Controls (enables/disables) the WiFi interface (STA or AP).  Use
    /// [`Self::wait_for_control_update`] to wait for a notification that the
    /// action has been applied.
    ///
    /// Arguments:
    /// - `wifi_type`: The type of WiFi interface to control (STA or AP)
    /// - `control`: The control action to perform (Enable or Disable)
    pub fn control(&self, wifi_type: WifiType, control: Control) {
        match wifi_type {
            WifiType::Sta => CONTROL_STA.signal(control),
            WifiType::Ap => CONTROL_AP.signal(control),
        }
    }

    /// Controls (enables/disables) the WiFi interface (STA or AP) and waits
    /// for the control update to be applied.  This is a convenience method
    /// that combines [`Self::control`] and [`Self::wait_for_control_update`].
    ///
    /// Arguments:
    /// - `wifi_type`: The type of WiFi interface to control (STA or AP)
    /// - `control`: The control action to perform (Enable or Disable)
    pub async fn control_and_wait(&self, wifi_type: WifiType, control: Control) -> Status {
        self.control(wifi_type, control);
        self.wait_for_control_update(wifi_type).await
    }

    /// Gets the networking stack for the specified WiFi type.
    ///
    /// Arguments:
    /// - `wifi_type`: The type of WiFi interface to get the stack for (STA or
    ///   AP)
    ///
    /// Returns:
    /// - `Some(Stack)` if the stack is configured for the specified WiFi type
    /// - `None` if the stack is not configured for the specified WiFi type
    pub fn net_stack(&self, wifi_type: WifiType) -> Option<Stack<'static>> {
        match wifi_type {
            WifiType::Sta => self.sta_stack,
            WifiType::Ap => self.ap_stack,
        }
    }

    /// Waits for a network stack link up status for the specified WiFi type
    ///
    /// Arguments:
    /// - `wifi_type`: The type of WiFi interface to wait for (STA or AP)
    ///
    /// Returns:
    /// - `Ok(())` when the link is up
    /// - `Err(Error::Config)` if the network stack for the specified WiFi type
    ///   is not configured
    pub async fn wait_for_link_up(&self, wifi_type: WifiType) -> Result<(), Error> {
        let net_stack = match wifi_type {
            WifiType::Sta => self.sta_stack.as_ref(),
            WifiType::Ap => self.ap_stack.as_ref(),
        }
        .ok_or(Error::Config(format!(
            "Network stack for WiFi {wifi_type:?} not configured"
        )))?;
        wait_for_wifi_connection(net_stack).await;
        Ok(())
    }

    /// Waits for an IP address to be assigned for the specified WiFi type.
    /// Useful when using DHCP to obtain an IP address.
    ///
    /// Arguments:
    /// - `wifi_type`: The type of WiFi interface to wait for (STA or AP)
    ///
    /// Returns:
    /// - `Ok(config)` the static IP address configuration when an IP address
    ///   is assigned
    /// - `Err(Error::Config)` if the network stack for the specified WiFi type
    ///   is not configured
    pub async fn wait_for_ipv4(&self, wifi_type: WifiType) -> Result<StaticConfigV4, Error> {
        let net_stack = match wifi_type {
            WifiType::Sta => self.sta_stack.as_ref(),
            WifiType::Ap => self.ap_stack.as_ref(),
        }
        .ok_or(Error::Config(format!(
            "Network stack for WiFi {wifi_type:?} not configured"
        )))?;
        Ok(wait_for_ipv4(net_stack).await)
    }
}

// STA interface events used by `sta_future()`.
enum StaEvent {
    Connected,
    Disconnected,
}

// Future to handle connecting to or waiting for disconnect from the station
// interface.  Having a single async function allows this call to be put in a
// single select arm.
//
// This function also signals the status of the STA interface when connects/
// disconnects happen.
async fn sta_future(
    controller: &mut WifiController<'_>,
    wifi_mode: Option<WifiMode>,
    sta_connected: bool,
) -> StaEvent {
    if let Some(wifi_mode) = wifi_mode
        && matches!(wifi_mode, WifiMode::Sta | WifiMode::ApSta)
    {
        if !sta_connected {
            info!("Exec:  Connecting WiFi station");
            match controller.connect_async().await {
                Ok(()) => {
                    STATUS_STA.signal(Status::Connected);
                    StaEvent::Connected
                }
                Err(_) => StaEvent::Disconnected,
            }
        } else {
            controller
                .wait_for_all_events(WifiEvent::StaDisconnected.into(), false)
                .await;
            warn!("Warn:  WiFi station disconnected");
            STATUS_STA.signal(Status::Disconnected);
            StaEvent::Disconnected
        }
    } else {
        pending().await
    }
}

// Handles starting and stopping STA and AP interfaces on demand.
#[embassy_executor::task]
async fn wifi_controller(mut controller: WifiController<'static>) -> ! {
    debug!(
        "Info:  WiFi device capabilities: {:?}",
        controller.capabilities()
    );

    let mut wifi_mode: Option<WifiMode> = None;
    let mut sta_connected = false;

    loop {
        // Single select to detect:
        // - Signal to enable/disable STA
        // - Signal to enable/disable AP
        // - Event from the STA interface (connected/disconnected)
        let (control, wifi_type) = match select3(
            CONTROL_STA.wait(),
            CONTROL_AP.wait(),
            sta_future(&mut controller, wifi_mode, sta_connected),
        )
        .await
        {
            Either3::First(control) => (control, WifiType::Sta),
            Either3::Second(control) => (control, WifiType::Ap),
            Either3::Third(event) => {
                match event {
                    StaEvent::Connected => sta_connected = true,
                    StaEvent::Disconnected => sta_connected = false,
                }
                continue;
            }
        };
        debug!("Info:  WiFi control signal received: {control:?} {wifi_type:?}",);

        // If we go there, a new WiFi mode has been requestd - figure out what
        let new_wifi_mode = match control {
            Control::Enable => enable_mode(wifi_mode, wifi_type),
            Control::Disable => disable_mode(wifi_mode, wifi_type),
        };
        debug!("Info:  Old WiFi mode {wifi_mode:?} new WiFi mode: {new_wifi_mode:?}");

        // If the WiFi mode has changed, reconfigure the WiFi controller
        if new_wifi_mode != wifi_mode {
            debug!("Info:  WiFi mode changed, reconfiguring");

            let result = reconfigure_wifi(&mut controller, new_wifi_mode, control, wifi_type)
                .await
                .inspect_err(|e| {
                    warn!("Error: Failed to reconfigure WiFi: {e}");
                });
            if result.is_err() {
                continue;
            }
        } else {
            warn!("Warning: WiFi mode did not change, ignoring control signal");
        };

        wifi_mode = new_wifi_mode;
    }
}

// Figures out what combination of interfaces is required when an enable
// control signal is received.
fn enable_mode(current: Option<WifiMode>, target: WifiType) -> Option<WifiMode> {
    match (current, target) {
        (None, WifiType::Sta) => Some(WifiMode::Sta),
        (None, WifiType::Ap) => Some(WifiMode::Ap),
        (Some(WifiMode::Sta), WifiType::Ap) => Some(WifiMode::ApSta),
        (Some(WifiMode::Ap), WifiType::Sta) => Some(WifiMode::ApSta),
        (current, _) => current, // Already enabled
    }
}

// Figures out what combination of interfaces is required when a disable
// control signal is received.
fn disable_mode(current: Option<WifiMode>, target: WifiType) -> Option<WifiMode> {
    match (current, target) {
        (Some(WifiMode::Sta), WifiType::Sta) => None,
        (Some(WifiMode::Ap), WifiType::Ap) => None,
        (Some(WifiMode::ApSta), WifiType::Sta) => Some(WifiMode::Ap),
        (Some(WifiMode::ApSta), WifiType::Ap) => Some(WifiMode::Sta),
        (current, _) => current, // Already disabled or not applicable
    }
}

// Perform the requested WiFi reconfiguration
async fn reconfigure_wifi(
    controller: &mut WifiController<'static>,
    new_wifi_mode: Option<WifiMode>,
    control: Control,
    wifi_type: WifiType,
) -> Result<(), Error> {
    // Stop the controller
    match controller.is_started() {
        Ok(true) => {
            info!("Exec:  Stopping WiFi for reconfiguration");
            match controller.stop_async().await {
                Ok(_) => debug!("Ok:    WiFi stopped"),
                Err(e) => return Err(Error::Wifi(format!("Failed to stop WiFi: {e:?}"))),
            }
        }
        Ok(false) => trace!("Info:  WiFi already stopped"),
        Err(e) => return Err(Error::Wifi(format!("Failed to check WiFi state: {e:?}"))),
    }

    // New mode is Some
    if let Some(new_wifi_mode) = new_wifi_mode {
        // Reconfigure it
        match controller.set_mode(new_wifi_mode) {
            Ok(()) => debug!("Ok:    WiFi mode set to {new_wifi_mode:?}"),
            Err(e) => return Err(Error::Wifi(format!("Failed to set WiFi mode: {e:?}"))),
        }

        // Start the controller
        match controller.start_async().await {
            Ok(_) => info!("Ok:    WiFi started in mode {new_wifi_mode:?}"),
            Err(e) => return Err(Error::Wifi(format!("Failed to start WiFi: {e:?}"))),
        }
    } else {
        debug!("Info:  WiFi mode disabled, not starting controller");
    }

    match (control, wifi_type) {
        (Control::Enable, WifiType::Sta) => STATUS_STA.signal(Status::Enabled),
        (Control::Disable, WifiType::Sta) => STATUS_STA.signal(Status::Disabled),
        (Control::Enable, WifiType::Ap) => STATUS_AP.signal(Status::Enabled),
        (Control::Disable, WifiType::Ap) => STATUS_AP.signal(Status::Disabled),
    }

    Ok(())
}

// Pool size of 2 required, one for STA, one for AP
#[embassy_executor::task(pool_size = 2)]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) -> ! {
    runner.run().await
}

/// This function waits for a WiFi connection
async fn wait_for_wifi_connection(net_stack: &Stack<'static>) {
    loop {
        if net_stack.is_link_up() {
            break;
        }
        Timer::after_millis(100).await;
    }
}

/// This function waits for an IP address to be assigned
async fn wait_for_ipv4(net_stack: &Stack<'static>) -> StaticConfigV4 {
    loop {
        // Wait for the network stack to receive valid IP configuration
        net_stack.wait_config_up().await;
        if let Some(config) = net_stack.config_v4() {
            info!("OK:    Received IP {}", config.address);
            return config;
        }
        Timer::after_millis(100).await;
    }
}
