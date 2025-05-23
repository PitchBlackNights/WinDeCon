// Partially adapted from:
// https://github.com/Valkirie/HandheldCompanion/blob/0503468f0388f5e7dd2d9e4390098ffb08ee0a15/hidapi.net/HidDevice.cs
// Licensing shouldn't be an issue (hopefully) because this is incomplete and will likely be completely replaced.

use crate::prelude::*;
use crate::set_priority;
use rusb::{
    ConfigDescriptor, Context, Device, DeviceHandle, DeviceList, Direction, Error as UsbError,
    UsbContext,
};
use std::sync::{Arc, Mutex, MutexGuard};
use std::{error::Error, thread::{self, JoinHandle}, time::Duration};

pub struct HidDevice {
    context: Arc<Mutex<Context>>,
    handle: Option<Arc<Mutex<DeviceHandle<Context>>>>,
    vid: u16,
    pid: u16,
    config: u8,
    interface: u8,
    setting: u8,
    endpoint: u8,
    input_buffer_len: usize,
    control_buffer_len: usize,
    on_input_received: Option<Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>>,
    read_thread: Option<JoinHandle<()>>,
    event_thread: Option<JoinHandle<()>>,
    stop_flag: Arc<Mutex<bool>>,
    active: bool,
}

impl HidDevice {
    pub fn new(vid: u16, pid: u16) -> Result<Self, UsbError> {
        let context = Context::new()?;
        Ok(Self {
            context: Arc::new(Mutex::new(context)),
            handle: None,
            vid: vid,
            pid: pid,
            config: 0,
            interface: 0,
            setting: 0,
            endpoint: 0x00,
            input_buffer_len: 64,
            control_buffer_len: 64,
            on_input_received: None,
            read_thread: None,
            event_thread: None,
            stop_flag: Arc::new(Mutex::new(false)),
            active: false,
        })
    }

    pub fn open(&mut self) -> Result<(), UsbError> {
        let devices: DeviceList<Context> = self.context.lock().unwrap().devices()?;
        let device: Device<Context> = devices
            .iter()
            .find(|d| {
                if let Ok(desc) = d.device_descriptor() {
                    desc.vendor_id() == self.vid && desc.product_id() == self.pid
                } else {
                    false
                }
            })
            .ok_or(UsbError::NoDevice)?;
        let handle: DeviceHandle<Context> = device.open()?;

        // Grab the correct interface & input endpoint address
        let mut found_interface: bool = false;
        let config_desc: ConfigDescriptor = device.config_descriptor(0)?;
        'out: for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                // Proceed only if interface is an HID interface
                if interface_desc.class_code() == 0x03 {
                    for endpoint_desc in interface_desc.endpoint_descriptors() {
                        // Accept the interface & endpoint if they meet the requirements for the Controller Data Input
                        if endpoint_desc.max_packet_size() == self.input_buffer_len as u16
                            && endpoint_desc.direction() == Direction::In
                        {
                            self.config = config_desc.number();
                            self.interface = interface_desc.interface_number();
                            self.setting = interface_desc.setting_number();
                            self.endpoint = endpoint_desc.address();
                            found_interface = true;
                            break 'out;
                        }
                    }
                }
            }
        }

        // If no matching interface was found, return an Error
        // This ensures that `self.interface` and `self.endpoint` are always valid values
        if !found_interface {
            return Err(UsbError::Other);
        }

        debug!("Device Handle info:");
        debug!("  VID: {:#04x}", self.vid,);
        debug!("  PID: {:#04x}", self.pid,);
        debug!("  Config: {}", self.config);
        debug!("  Interface: {}", self.interface,);
        debug!("  Setting: {}", self.setting);
        debug!("  Endpoint: {:#02x}", self.endpoint,);

        debug!("Setting Active Config to {}...", self.config);
        handle.set_active_configuration(self.config)?;
        debug!("Active Config was set");

        debug!("Claiming Interface {}...", self.interface);
        handle.claim_interface(self.interface)?;
        debug!("Interface was claimed");

        debug!(
            "Setting Interface Settings ({}, {})",
            self.interface, self.setting
        );
        handle.set_alternate_setting(self.interface, self.setting)?;
        debug!("Interface Settings were set");

        self.handle = Some(Arc::new(Mutex::new(handle)));
        self.active = true;

        self.begin_handle_events();
        self.begin_read();
        debug!("Device successfully opened");
        Ok(())
    }

    /// May panic if the read thread encounters a problem joining the main thread
    pub fn close(&mut self) -> Result<(), UsbError> {
        if !self.active {
            return Err(UsbError::NoDevice);
        }

        // Drop the handle to close the device
        self.handle = None;
        // Signal read thread to stop
        *self.stop_flag.lock().unwrap() = true;
        if let Some(handle) = self.event_thread.take() {
            handle.join().ok();
            trace!("Exited thread `event_loop`");
        }
        if let Some(handle) = self.read_thread.take() {
            handle.join().ok();
            trace!("Exited thread `read_loop`");
        }
        Ok(())
    }

    pub fn read(&self) -> Result<(usize, Vec<u8>), UsbError> {
        if !self.active {
            return Err(UsbError::NoDevice);
        }

        let handle: MutexGuard<'_, DeviceHandle<Context>> =
            self.handle.as_ref().unwrap().lock().unwrap();
        let has_kernel_driver = match handle.kernel_driver_active(self.interface) {
            Ok(true) => {
                handle.detach_kernel_driver(self.interface).ok();
                true
            }
            _ => false,
        };
        debug!(
            "Interface {} has kernel driver? {}",
            self.interface, has_kernel_driver
        );

        let mut buf: Vec<u8> = vec![0u8; self.input_buffer_len];
        let len: usize =
            handle.read_interrupt(self.endpoint, &mut buf, Duration::from_millis(100))?;

        if has_kernel_driver {
            handle.attach_kernel_driver(self.interface).ok();
        }

        Ok((len, buf[..len].to_vec()))
    }


    /// HID Feature reports are sent via control transfer on endpoint 0
    // BUG: Currently the Control Transfers always throw an error: Io
    // https://github.com/libusb/libusb/blob/ed09a92b0b39fa906bf964a50a8b8a8c27c09877/libusb/sync.c#L161
    // ^^^ An Io error is caused by LIBUSB_TRANSFER_ERROR or LIBUSB_TRANSFER_CANCELLED, except I have no idea which or why.
    // Stupid Unhelpful Vague Overcomplicated Errors. This works on Handheld Companion, why not here! I am literally copying the exact packets sent by HC.
    pub fn request_feature_report(
        &self,
        request: &[u8],
    ) -> Result<(usize, Vec<u8>), Box<dyn Error>> {
        if !self.active {
            return Err(Box::new(UsbError::NoDevice));
        } else if request.len() > self.control_buffer_len {
            return Err("Request length is greater than control buffer length".into());
        }

        let handle: MutexGuard<'_, DeviceHandle<Context>> =
            self.handle.as_ref().unwrap().lock().unwrap();
        let mut request_full: Vec<u8> = vec![0u8; self.control_buffer_len];
        request_full[..request.len()].copy_from_slice(request);

        // Send feature report (SET_REPORT)
        let request_type: u8 = rusb::request_type(
            rusb::Direction::Out,
            rusb::RequestType::Class,
            rusb::Recipient::Interface,
        );
        debug!("====== WRITE USBHID PACKET ======");
        debug!("  bmRequestType: {:#02x}", request_type);
        debug!("  bRequest: 0x09");
        debug!("  wValue: 0x0300");
        debug!("  wIndex: {}", self.interface as u16);
        debug!("  Data ({}): {:02x?}", request_full.len(), request_full);
        debug!("  Timeout: 100ms");
        debug!("====== WRITE USBHID PACKET ======");
        debug!("Sending \"Write Control Transfer\" packet...");
        handle.write_control(
            // bmRequestType: 0x21  --  The request type
            // 0... .... = Direction: Host-to-device (Out)
            // .01. .... = Type: Class (0x1)
            // ...0 0001 = Recipient: Interface (0x1)
            request_type,
            // bRequest: SET_REPORT (0x09)  --  The request function
            0x09,
            // wValue: 0x0300  --  Idk what this specific value means
            0x0300,
            // wIndex: 2  --  Specifies the interface number to send the packet to
            self.interface as u16,
            // Data  --  The data to send to the device
            &request_full,
            Duration::from_millis(100),
        )?;
        debug!("\"Write Control Transfer\" succeeded");

        // Get feature report (GET_REPORT)
        let mut response: Vec<u8> = vec![0u8; self.control_buffer_len];
        let request_type: u8 = rusb::request_type(
            rusb::Direction::In,
            rusb::RequestType::Class,
            rusb::Recipient::Interface,
        );
        debug!("====== READ USBHID PACKET ======");
        debug!("  bmRequestType: {:#02x}", request_type);
        debug!("  bRequest: 0x09");
        debug!("  wValue: 0x0300");
        debug!("  wIndex: {}", self.interface as u16);
        debug!("  Data ({}): {:02x?}", request_full.len(), request_full);
        debug!("  Timeout: 100ms");
        debug!("====== READ USBHID PACKET ======");
        debug!("Sending \"Read Control Transfer\" packet...");
        let len = handle.read_control(
            // bmRequestType: 0xa1  --  The request type
            // 1... .... = Direction: Device-to-host (In)
            // .01. .... = Type: Class (0x1)
            // ...0 0001 = Recipient: Interface (0x1)
            request_type,
            // bRequest: GET_REPORT (0x01)  --  The request function
            0x01,
            // wValue: 0x0300  --  Idk what this specific value means
            0x0300,
            // wIndex: 2  --  Specifies the interface number to send the packet to
            self.interface as u16,
            // Data  --  The data to send to the device
            &mut response,
            Duration::from_millis(100),
        )?;
        debug!("\"Read Control Transfer\" succeeded");

        response.truncate(len);
        Ok((len, response))
    }

    pub fn set_on_input_received<F>(&mut self, callback: F)
    where
        F: Fn(Vec<u8>) + Send + Sync + 'static,
    {
        self.on_input_received = Some(Arc::new(callback));
    }

    // Idk if this is actually needed... but trying it shouldn't hurt, right?
    fn begin_handle_events(&mut self) {
        // `HidDevice::begin_handle_events()` is only used in `HidDevice::open()` after the device is successfully opened,
        // this means that checking `self.active` is not required.

        let context: Arc<Mutex<Context>> = self.context.clone();
        let stop_flag: Arc<Mutex<bool>> = self.stop_flag.clone();

        trace!("Entering thread `event_loop`...");
        self.event_thread = Some(
            thread::Builder::new()
                .name("event_loop".into())
                .spawn(move || {
                    set_priority!(highest);
                    trace!("Entered thread");

                    loop {
                        if *stop_flag.lock().unwrap() {
                            break;
                        }
                        context.lock().unwrap().handle_events(None).unwrap();
                    }

                    trace!("Exiting thread...");
                })
                .unwrap()
        );
    }

    fn begin_read(&mut self) {
        // `HidDevice::begin_read()` is only used in `HidDevice::open()` after the device is successfully opened,
        // this means that checking `self.active` is not required.

        let handle: Arc<Mutex<DeviceHandle<Context>>> = self.handle.as_ref().unwrap().clone();
        let input_buffer_len: usize = self.input_buffer_len;
        let on_input_received: Option<Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>> =
            self.on_input_received.clone();
        let stop_flag: Arc<Mutex<bool>> = self.stop_flag.clone();
        let interface: u8 = self.interface;
        let endpoint: u8 = self.endpoint;

        trace!("Entering thread `read_loop`...");
        self.read_thread = Some(
            thread::Builder::new()
                .name("read_loop".into())
                .spawn(move || {
                    set_priority!(highest);
                    trace!("Entered thread");

                    let blank_buffer: Vec<u8> = vec![0u8; input_buffer_len];
                    let mut buffer: Vec<u8> = blank_buffer.clone();
                    loop {
                        if *stop_flag.lock().unwrap() {
                            break;
                        }
                        let handle: MutexGuard<'_, DeviceHandle<Context>> = handle.lock().unwrap();
                        let has_kernel_driver = match handle.kernel_driver_active(interface) {
                            Ok(true) => {
                                handle.detach_kernel_driver(interface).ok();
                                true
                            }
                            _ => false,
                        };
                        // debug!(
                        //     "Interface {} has kernel driver? {}",
                        //     interface, has_kernel_driver
                        // );

                        match handle.read_interrupt(
                            endpoint,
                            &mut buffer,
                            Duration::from_millis(100),
                        ) {
                            Ok(len) if len > 0 => {
                                if let Some(ref cb) = on_input_received {
                                    cb(buffer[..len].to_vec());
                                }
                            }
                            _ => {}
                        }

                        if has_kernel_driver {
                            handle.attach_kernel_driver(interface).ok();
                        }

                        buffer = blank_buffer.clone();
                        drop(handle);
                    }

                    trace!("Exiting thread...");
                })
                .unwrap(),
        );
    }
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        if self.active {
            self.close().unwrap();
        }
    }
}
