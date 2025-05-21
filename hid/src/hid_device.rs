// Partially adapted from:
// https://github.com/Valkirie/HandheldCompanion/blob/0503468f0388f5e7dd2d9e4390098ffb08ee0a15/hidapi.net/HidDevice.cs
// Licensing shouldn't be an issue (hopefully) because this is incomplete and will likely be completely replaced.

use rusb::{DeviceHandle, GlobalContext};
use std::{
    io,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

pub struct HidDevice {
    vid: u16,
    pid: u16,
    input_buffer_len: usize,
    handle: Option<Arc<Mutex<DeviceHandle<GlobalContext>>>>,
    interface: u8,
    endpoint_in: u8,
    endpoint_out: u8,
    pub reading: Arc<Mutex<bool>>,
    pub on_input_received: Option<Arc<Mutex<dyn Fn(Vec<u8>) + Send + Sync>>>,
}

impl HidDevice {
    pub fn new(vid: u16, pid: u16, input_buffer_len: usize) -> Self {
        Self {
            vid: vid,
            pid: pid,
            input_buffer_len: input_buffer_len,
            handle: None,
            interface: 0,
            endpoint_in: 0x81,  // Default HID in endpoint, may need adjustment
            endpoint_out: 0x01, // Default HID out endpoint, may need adjustment
            reading: Arc::new(Mutex::new(false)),
            on_input_received: None,
        }
    }

    pub fn open(&mut self) -> rusb::Result<()> {
        for device in rusb::devices()?.iter() {
            let desc = device.device_descriptor()?;
            if desc.vendor_id() == self.vid && desc.product_id() == self.pid {
                let handle = device.open()?;

                // Find HID interface and endpoints
                let config = device.active_config_descriptor()?;
                for interface in config.interfaces() {
                    for interface_desc in interface.descriptors() {
                        // 0x03 is HID
                        if interface_desc.class_code() == 0x03 {
                            self.interface = interface_desc.interface_number();
                            for endpoint in interface_desc.endpoint_descriptors() {
                                if endpoint.direction() == rusb::Direction::In {
                                    self.endpoint_in = endpoint.address();
                                } else {
                                    self.endpoint_out = endpoint.address();
                                }
                            }
                        }
                    }
                }

                handle.claim_interface(self.interface)?;
                self.handle = Some(Arc::new(Mutex::new(handle)));
                return Ok(());
            }
        }
        Err(rusb::Error::NoDevice)
    }

    pub fn close(&mut self) {
        if let Some(handle) = &mut self.handle {
            let _ = handle.lock().unwrap().release_interface(self.interface);
        }
        self.handle = None;
    }

    pub fn is_valid(&self) -> bool {
        self.handle.is_some()
    }

    pub fn read(&mut self, timeout_ms: u64) -> io::Result<Vec<u8>> {
        if let Some(handle_arc) = &mut self.handle {
            let handle = handle_arc.lock().unwrap();
            let mut buf = vec![0u8; self.input_buffer_len];
            let bytes_read = handle
                .read_interrupt(
                    self.endpoint_in,
                    &mut buf,
                    Duration::from_millis(timeout_ms),
                )
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            buf.truncate(bytes_read);
            Ok(buf)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Device not open",
            ))
        }
    }

    pub fn write(&mut self, data: &[u8], timeout_ms: u64) -> io::Result<usize> {
        if data.len() > self.input_buffer_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Data length exceeds input buffer length",
            ));
        }
        if let Some(handle_arc) = &mut self.handle {
            let handle = handle_arc.lock().unwrap();
            let mut buf = vec![0u8; self.input_buffer_len];
            buf[..data.len()].copy_from_slice(data);
            handle
                .write_interrupt(self.endpoint_out, &buf, Duration::from_millis(timeout_ms))
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Device not open",
            ))
        }
    }

    pub fn request_feature_report(&mut self, request: &[u8]) -> io::Result<Vec<u8>> {
        if request.len() > self.input_buffer_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Request length is greater than input buffer length.",
            ));
        }
        let handle = self
            .handle
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Device not open"))?;

        if let Some(handle_arc) = &mut self.handle {
            let handle = handle_arc.lock().unwrap();
            // Prepare request buffer [report_id | ...request...]
            let mut request_full = vec![0u8; self.input_buffer_len + 1];
            request_full[1..1 + request.len()].copy_from_slice(request);

            // Send feature report (SET_REPORT)
            let req_type = rusb::request_type(
                rusb::Direction::Out,
                rusb::RequestType::Class,
                rusb::Recipient::Interface,
            );
            let request_code = 0x09; // SET_REPORT
            let value = (3 << 8) | (request_full[0] as u16); // 3 = Feature report, report ID

            let _ = handle
                .write_control(
                    req_type,
                    request_code,
                    value,
                    self.interface as u16,
                    &request_full,
                    Duration::from_millis(1000),
                )
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

            // Prepare response buffer [report_id | ...response...]
            let mut response = vec![0u8; self.input_buffer_len + 1];
            let req_type_in = rusb::request_type(
                rusb::Direction::In,
                rusb::RequestType::Class,
                rusb::Recipient::Interface,
            );
            let request_code_in = 0x01; // GET_REPORT
            let value_in = (3 << 8) | (request_full[0] as u16); // 3 = Feature report, report ID

            let len = handle
                .read_control(
                    req_type_in,
                    request_code_in,
                    value_in,
                    self.interface as u16,
                    &mut response,
                    Duration::from_millis(1000),
                )
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

            response.truncate(len);
            Ok(response)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Device not open",
            ))
        }
    }

    /// Start a background thread reading from the device and calling the callback on each packet
    pub fn begin_read(&mut self) {
        let reading = Arc::clone(&self.reading);
        *reading.lock().unwrap() = true;
        let handle_arc = self.handle.as_ref().unwrap().clone();
        let endpoint_in = self.endpoint_in;
        let input_buffer_len = self.input_buffer_len;
        let cb_opt = self.on_input_received.clone();
        thread::spawn(move || {
            unsafe {
                drop(SetThreadPriority(
                    GetCurrentThread(),
                    THREAD_PRIORITY_HIGHEST,
                ));
            }
            while *reading.lock().unwrap() {
                let mut buf = vec![0u8; input_buffer_len];
                let handle = handle_arc.lock().unwrap();
                match handle.read_interrupt(endpoint_in, &mut buf, Duration::from_millis(1000)) {
                    Ok(len) => {
                        buf.truncate(len);
                        if let Some(cb) = &cb_opt {
                            let cb = cb.lock().unwrap();
                            cb(buf);
                        }
                    }
                    Err(_) => {
                        drop(handle);
                        thread::sleep(Duration::from_millis(10))
                    }
                }
            }
        });
    }

    pub fn end_read(&mut self) {
        *self.reading.lock().unwrap() = false;
    }

    // Set the callback
    pub fn set_input_callback<F>(&mut self, callback: F)
    where
        F: Fn(Vec<u8>) + Send + Sync + 'static,
    {
        self.on_input_received = Some(Arc::new(Mutex::new(callback)));
    }
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        self.close();
    }
}
