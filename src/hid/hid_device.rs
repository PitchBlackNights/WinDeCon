// Partially adapted from:
// https://github.com/Valkirie/HandheldCompanion/blob/0503468f0388f5e7dd2d9e4390098ffb08ee0a15/hidapi.net/HidDevice.cs
// Licensing shouldn't be an issue (hopefully) because this is incomplete and will likely be completely replaced.

use rusb::{
    Context,
    ConfigDescriptor,
    Device,
    DeviceList,
    DeviceHandle,
    Direction,
    Error as UsbError,
    UsbContext
};
use std::sync::{Arc, Mutex, MutexGuard};
use std::{error::Error, thread, time::Duration};
use crate::prelude::*;

pub struct HidDevice {
    context: Arc<Context>,
    handle: Option<Arc<Mutex<DeviceHandle<Context>>>>,
    vid: u16,
    pid: u16,
    interface: u8,
    input_endpoint: u8,
    input_buffer_len: usize,
    control_buffer_len: usize,
    on_input_received: Option<Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>>,
    read_thread: Option<thread::JoinHandle<()>>,
    stop_flag: Arc<Mutex<bool>>,
    active: bool,
}

impl HidDevice {
    pub fn new(vid: u16, pid: u16) -> Result<Self, UsbError> {
        let context = Context::new()?;
        Ok(Self {
            context: Arc::new(context),
            handle: None, // After successfully opening the device, `handle` will always be valid, so setting as None has no effect
            vid: vid,
            pid: pid,
            interface: 0, // After successfully opening the device, `interface` will always be valid, so setting as 0 has no effect.
            input_endpoint: 0x00, // After successfully opening the device, `input_endpoint` will always be valid, so setting as 0x00 has no effect.
            input_buffer_len: 64,
            control_buffer_len: 64,
            on_input_received: None,
            read_thread: None,
            stop_flag: Arc::new(Mutex::new(false)),
            active: false,
        })
    }

    pub fn open(&mut self) -> Result<(), UsbError> {
        let devices: DeviceList<Context> = self.context.devices()?;
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
                            self.interface = interface_desc.interface_number();
                            self.input_endpoint = endpoint_desc.address();
                            found_interface = true;
                            break 'out;
                        }
                    }
                }
            }
        }

        // If no matching interface was found, return an Error
        // This ensures that `self.interface` and `self.input_endpoint` are always valid values
        if !found_interface {
            return Err(UsbError::Other);
        }

        debug!(
            "VID: {:#04x}\nPID: {:#04x}\nInterface: {}\nInput Endpoint: {:#02x}",
            self.vid, self.pid, self.interface, self.input_endpoint,
        );

        debug!("Trying to claim interface: {}", self.interface);
        handle.claim_interface(self.interface)?;
        debug!("Claimed interface: {}", self.interface);
        self.handle = Some(Arc::new(Mutex::new(handle)));
        self.active = true;
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
        if let Some(handle) = self.read_thread.take() {
            *self.stop_flag.lock().unwrap() = true;
            handle.join().ok();
        }
        Ok(())
    }

    pub fn read(&self) -> Result<(usize, Vec<u8>), UsbError> {
        if !self.active {
            return Err(UsbError::NoDevice);
        }

        let handle: MutexGuard<'_, DeviceHandle<Context>> = self.handle.as_ref().unwrap().lock().unwrap();
        let mut buf: Vec<u8> = vec![0u8; self.input_buffer_len];
        let len: usize =
            handle.read_interrupt(self.input_endpoint, &mut buf, Duration::from_millis(100))?;
        // buf.truncate(len);
        Ok((len, buf))
    }

    /// HID Feature reports are sent via control transfer on endpoint 0
    pub fn request_feature_report(
        &self,
        request: &[u8; 16],
    ) -> Result<(usize, Vec<u8>), Box<dyn Error>> {
        if !self.active {
            return Err(Box::new(UsbError::NoDevice));
        } else if request.len() > self.control_buffer_len {
            return Err("Request length is greater than control buffer length".into());
        }

        let handle: MutexGuard<'_, DeviceHandle<Context>> = self.handle.as_ref().unwrap().lock().unwrap();
        let mut request_full: Vec<u8> = vec![0u8; self.control_buffer_len + 1];
        request_full[1..1 + request.len()].copy_from_slice(request);

        // Send feature report (SET_REPORT)
        let request_type: u8 = rusb::request_type(
            rusb::Direction::Out,
            rusb::RequestType::Class,
            rusb::Recipient::Interface,
        );
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

        // Get feature report (GET_REPORT)
        let request_type: u8 = rusb::request_type(
            rusb::Direction::In,
            rusb::RequestType::Class,
            rusb::Recipient::Interface,
        );
        let mut response: Vec<u8> = vec![0u8; self.control_buffer_len + 1];
        let len = handle.read_control(
            request_type,
            0x01,   // HID Get_Report
            0x0300, // 3 = Feature
            self.interface as u16,
            &mut response,
            Duration::from_millis(100),
        )?;
        // response.truncate(len);
        Ok((len, response))
    }

    pub fn set_on_input_received<F>(&mut self, callback: F)
    where
        F: Fn(Vec<u8>) + Send + Sync + 'static,
    {
        self.on_input_received = Some(Arc::new(callback));
    }

    fn begin_read(&mut self) {
        let handle: Arc<Mutex<DeviceHandle<Context>>> = self.handle.as_ref().unwrap().clone();
        let input_buffer_len: usize = self.input_buffer_len;
        let on_input_received: Option<Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>> = self.on_input_received.clone();
        let stop_flag: Arc<Mutex<bool>> = Arc::clone(&self.stop_flag);
        let input_endpoint = self.input_endpoint;

        self.read_thread = Some(thread::spawn(move || {
            unsafe {
                drop(windows::Win32::System::Threading::SetThreadPriority(
                    windows::Win32::System::Threading::GetCurrentThread(),
                    windows::Win32::System::Threading::THREAD_PRIORITY_HIGHEST,
                ));
            }

            let mut buffer: Vec<u8> = vec![0u8; input_buffer_len];
            loop {
                if *stop_flag.lock().unwrap() {
                    break;
                }
                let handle: MutexGuard<'_, DeviceHandle<Context>> = handle.lock().unwrap();
                match handle.read_interrupt(input_endpoint, &mut buffer, Duration::from_millis(100))
                {
                    Ok(len) if len > 0 => {
                        if let Some(ref cb) = on_input_received {
                            // let data = buffer[..len].to_vec();
                            cb(buffer.to_vec());
                        }
                    }
                    _ => {}
                }
                drop(handle);
            }
        }));
    }
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
