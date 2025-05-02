use libc::wcslen;
use widestring::U16String;
use windows::Win32::System::Environment::*;
use windows_core::*;

fn main() {
    let cmd = unsafe {
        let buf: PCWSTR = GetCommandLineW();
        let len = wcslen(buf.0);

        let wstr = U16String::from_ptr(buf.0, len);
        wstr.to_string_lossy()
    };
    println!("Hello, Windows! My full command line is: {}", cmd);
    println!("  args:");
    for (i, arg) in ::std::env::args().enumerate() {
        println!("  - [{}]: {:?}", i, arg);
    }
}
