use std::io::{self, Cursor};

use hexyl::*;

use win_high::prelude::v1::*;

fn main() {
    let buf = do_get_values().expect("Get values");
    xxd(&*buf).expect("Print hex value");
}

fn do_get_values() -> WinResult<Vec<u8>> {
    let hkey = RegConnectRegistryW_Safe(null(), HKEY_PERFORMANCE_DATA)?;

    // Retrieve counter data for the Processor object.
    query_value(
        *hkey,
        "238",
        Some(2_000_000)
    )
}

fn xxd(buffer: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = Cursor::new(buffer);
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let show_color = true;
    let border_style = BorderStyle::Unicode;
    let squeeze = false;

    let mut printer = Printer::new(&mut stdout_lock, show_color, border_style, squeeze);
    printer.display_offset(0);
    printer.print_all(&mut reader)
}