use std::mem::size_of;

use crate::prelude::v1::*;

const INITIAL_BUFFER_SIZE: usize = 8 * 1024;
// Abort after 1 GB
const MAX_BUFFER_SIZE: usize = 1024 * 1024 * 1024;

/// Create new buffer and call `query_value_with_buffer`.
pub fn query_value(
    hkey: HKEY,
    value_name: &str,
    value_type: Option<&mut DWORD>,
    buffer_size_hint: Option<usize>,
) -> WinResult<Vec<u8>>
{
    let mut buffer = Vec::new();
    query_value_with_buffer(
        hkey,
        value_name,
        value_type,
        buffer_size_hint,
        &mut buffer,
    )?;
    Ok(buffer)
}

pub fn query_value_dword(
    hkey: HKEY,
    value_name: &str,
) -> WinResult<DWORD> {
    let mut value_type = 0;
    let buffer = query_value(
        hkey,
        value_name,
        Some(&mut value_type),
        Some(size_of::<DWORD>()),
    )?;
    if (value_type & RRF_RT_REG_DWORD) != 0 {
        return Err(WinError::new(ERROR_INVALID_DATA)
            .with_comment(format!(
                "Unexpected data type in registry. Expected DWORD, got: {:#10x}", value_type)));
    }
    if buffer.len() != size_of::<DWORD>() {
        return Err(WinError::new(ERROR_INVALID_DATA)
            .with_comment(format!(
                "Unexpected buffer size for type DWORD. Expected: {}, got: {}", size_of::<DWORD>(), buffer.len())));
    }
    let num = {
        let mut bytes = [0; size_of::<DWORD>()];
        bytes.copy_from_slice(&buffer[..]);
        DWORD::from_ne_bytes(bytes)
    };
    Ok(num)
}

/// Query registry value of potentially unknown size, reallocating larger buffer in a loop as needed.
/// Given buffer will be cleared and overridden with zeroes before usage.
/// # Panics
/// Will panic if value_name contains NULL characters.
pub fn query_value_with_buffer(
    hkey: HKEY,
    value_name: &str,
    value_type: Option<&mut DWORD>,
    buffer_size_hint: Option<usize>,
    buffer: &mut Vec<u8>,
) -> WinResult<()>
{
    // prepare value name with trailing NULL char
    let wsz_value_name = U16CString::from_str(value_name).unwrap();
    let lp_type = value_type.map(|t| t as LPDWORD).unwrap_or(null_mut());

    // start with some non-zero size, even if explicit zero were provided, and gradually
    // increment it until value fits into buffer.
    // some queries may return undefined size and ERROR_MORE_DATA status when they don't know
    // the data size in advance.
    let mut buffer_size = match buffer_size_hint {
        Some(0) => {
            eprintln!("Explicit Some(0) size hint provided. Use None instead.");
            INITIAL_BUFFER_SIZE
        }
        Some(hint) => hint,
        None => {
            match try_get_size_hint(hkey, value_name, wsz_value_name.as_ref()) {
                Ok(Some(hint)) => hint as usize,
                Ok(None) => INITIAL_BUFFER_SIZE,
                // gracefully fallback to incremental buffer allocation, do not return error here.
                Err(why) => {
                    eprintln!("{}", why);
                    INITIAL_BUFFER_SIZE
                }
            }
        }
    };
    // From MSDN:
    // If hKey specifies HKEY_PERFORMANCE_DATA and the lpData buffer is not large enough to
    // contain all of the returned data, RegQueryValueEx returns ERROR_MORE_DATA and the value
    // returned through the lpcbData parameter is undefined.
    // [..]
    // You need to maintain a separate variable to keep track of the buffer size, because the
    // value returned by lpcbData is unpredictable.
    let mut buffer_size_out = buffer_size as DWORD;
    // buffer initialization
    buffer.clear();
    buffer.reserve(buffer_size);

    let mut error_code: DWORD;
    unsafe {
        error_code = RegQueryValueExW(
            hkey,
            wsz_value_name.as_ptr(),
            null_mut(),
            lp_type,
            buffer.as_mut_ptr(),
            &mut buffer_size_out as LPDWORD,
        ) as DWORD;

        while error_code == ERROR_MORE_DATA {
            // initialize buffer size or double its value
            let increment = if buffer_size == 0 { INITIAL_BUFFER_SIZE } else { buffer_size };
            buffer_size += increment;
            buffer_size_out = buffer_size as DWORD;
            if buffer_size > MAX_BUFFER_SIZE {
                return Err(WinError::new(ERROR_MORE_DATA)
                    .with_comment(format!("RegQueryValueExW reached buffer limit: {} bytes", buffer_size)));
            }
            // buffer considers itself empty, so reversing for "additional" N items is the same as
            // reserving for total of N items.
            buffer.reserve(buffer_size);

            // exactly same call as above
            error_code = RegQueryValueExW(
                hkey,
                wsz_value_name.as_ptr(),
                null_mut(),
                lp_type,
                buffer.as_mut_ptr(),
                &mut buffer_size_out as LPDWORD,
            ) as DWORD;
        }
    }

    if error_code != ERROR_SUCCESS {
        return Err(WinError::new_with_message(error_code)
            .with_comment(format!("RegQueryValueExW with query: {}", value_name)));
    }

    // SAFETY: buffer_size_out is initialized to a valid value by a successful call to RegQueryValueExW
    unsafe { buffer.set_len(buffer_size_out as usize) };
    Ok(())
}

/// in cases when value size is known in advance, we may __try__ to use that as a size hint.
/// but it won't work with dynamic values, such as counters data; and it must not be trusted
/// as a final size, because anything could happen between two calls to RegQueryValueExW.
///
/// it certainly can NOT be used under these conditions:
/// - HKEY is HKEY_PERFORMANCE_DATA but
/// - value_name is not starting with neither "Counter" nor "Help".
fn try_get_size_hint(hkey: HKEY, value_name: &str, wsz_value_name: &U16CStr) -> WinResult<Option<DWORD>> {
    let can_not_use_reg_size_hint =
        (hkey == HKEY_PERFORMANCE_DATA)
            && (!value_name.starts_with("Counter")
            && !value_name.starts_with("Help"));

    if can_not_use_reg_size_hint {
        return Ok(None);
    }

    let mut reg_size_hint: DWORD = 0;
    // pass NULL data to figure out needed buffer size
    let error_code = unsafe {
        RegQueryValueExW(
            hkey,
            wsz_value_name.as_ptr(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut reg_size_hint as LPDWORD,
        ) as DWORD
    };

    if error_code != ERROR_SUCCESS {
        return Err(WinError::new_with_message(error_code).with_comment(format!(
            "Getting buffer size hint for registry value failed. \
            This should not happen for HKEY = {:p}, ValueName = {:?}", hkey, value_name)));
    }

    Ok(Some(reg_size_hint))
}
