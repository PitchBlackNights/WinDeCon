use windecon::{hid, cli_parser::Args, prelude::*, setup};
use std::{thread, time::Duration};

const VID: u16 = 0x28DE;
const PID: u16 = 0x1205;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _args: Args = setup::setup_logger_and_args();
    info!("Starting WinDeCon...");

    let mut dev: hid::HidDevice = hid::HidDevice::new(VID, PID)?;

    // TODO: Why isn't this being called...
    dev.set_on_input_received(|data| {
        println!("INPUT RECEIVED: {:?}", data);
    });
    dev.open()?;
    thread::sleep(Duration::from_millis(1000));
    let _ = dev.close();

    // thread::spawn(move || {
    //     for cycle in 0..5 {
    //         // Set lizard mode to True
    //         println!("{:?}", dev.request_feature_report(&[0x85, 0x00]).unwrap()); // DEFAULT_MAPPING
    //         println!("{:?}", dev.request_feature_report(&[0x8E, 0x00]).unwrap()); // DEFAULT_MOUSE
    //         println!("Cycle {cycle} complete...");
    //         thread::sleep(Duration::from_millis(1000));
    //     }
    //     dev.close();
    // })
    // .join()
    // .unwrap();

    Ok(())
}
