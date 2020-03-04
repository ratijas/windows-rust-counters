extern crate libc;
extern crate windows_rust_test;

use libc::wcslen;

use windows_rust_test::win::uses::*;

fn main() {
    let cmd = unsafe {
        let buf: LPWSTR = winapi::um::processenv::GetCommandLineW();
        let len = wcslen(buf);

        let wstr = WideString::from_ptr(buf, len);
        wstr.to_string_lossy()
    };
    println!("Hello, Windows! My full command line is: {}", cmd);
    println!("  args:");
    for (i, arg) in ::std::env::args().enumerate() {
        println!("  - [{}]: {:?}", i, arg);
    }
}
