use std::sync::{Arc, Mutex, MutexGuard};
use std::{thread, time::Duration};
use windecon::{cli_parser::Args, hid, prelude::*, setup};

const VID: u16 = 0x28DE;
const PID: u16 = 0x1205;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _args: Args = setup::setup_logger_and_args();
    info!("Starting WinDeCon...");

    let dev: Arc<Mutex<hid::HidDevice>> = Arc::new(Mutex::new(hid::HidDevice::new(VID, PID)?));

    // BUG: Fix this not being called
    // Might be caused by heartbeat not being sent
    dev.lock().unwrap().set_on_input_received(|data| {
        debug!("INPUT RECEIVED: {:?}", data);
    });
    dev.lock().unwrap().open()?;

    let dev_clone: Arc<Mutex<hid::HidDevice>> = Arc::clone(&dev);
    thread::Builder::new()
        .name("heartbeat".into())
        .spawn(move || {
            for cycle in 0..1 {
                let dev: MutexGuard<'_, hid::HidDevice> = dev_clone.lock().unwrap();
                // Set lizard mode to True

                // DEFAULT_MAPPING
                match dev.request_feature_report(&[0x85, 0x00]) {
                    Ok((_, response)) => info!("Response 1: {:02x?}", response),
                    Err(err) => error!("Feature Report Request #1 returned an error: {:?}", err),
                }

                // DEFAULT_MOUSE
                match dev.request_feature_report(&[0x8E, 0x00]) {
                    Ok((_, response)) => info!("Response 2: {:02x?}", response),
                    Err(err) => error!("Feature Report Request #2 returned an error: {:?}", err),
                }

                info!("Cycle {cycle} complete...");
                thread::sleep(Duration::from_millis(1000));
            }
        })
        .unwrap()
        .join()
        .unwrap();

    info!("Closing!");
    dev.lock().unwrap().close().unwrap();

    Ok(())
}
