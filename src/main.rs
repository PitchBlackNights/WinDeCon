use hid;
use std::{thread, time::Duration};

const VID: u16 = 0x28DE;
const PID: u16 = 0x1205;

// Example usage:
// let mut dev = HidDevice::new(0x1234, 0xabcd, 64);
// dev.open().unwrap();
// let data = dev.read(1000).unwrap();
// dev.write(&[0x01, 0x02, 0x03], 1000).unwrap();

fn main() -> Result<(), &'static str> {
    let mut dev = hid::HidDevice::new(VID, PID, 64);
    dev.open().unwrap();
    if dev.is_valid() {
        dev.set_input_callback(|data| {
            println!("INPUT RECEIVED: {data:?}");
        });
        thread::spawn(move || {
            for cycle in 0..5 {
                // Set lizard mode to True
                let _ = dev.request_feature_report(&[0x85, 0x00]); // DEFAULT_MAPPINGS
                let _ = dev.request_feature_report(&[0x8E, 0x00]); // DEFAULT_MOUSE
                println!("Cycle {cycle} complete...");
                thread::sleep(Duration::from_millis(1000));
            }
        }).join().unwrap();
        Ok(())
    } else {
        Err("DEVICE IS NOT VALID!!!!")
    }
}
