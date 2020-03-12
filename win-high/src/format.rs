use crate::prelude::v1::*;

/// Commonly used raw byte string format uses 0u16 as a delimiter for UTF-16 strings
/// terminated by double 0u16 sequence.
pub fn parse_slice_null_delimited_double_null_terminated(input: &[u16]) -> Result<Vec<&U16CStr>, ()> {
    if !input.ends_with(&[u16::NUL; 2]) {
        return Err(());
    }
    Ok(unsafe { parse_ptr_null_delimited_double_null_terminated(input.as_ptr()) })
}

/// Usa safe wrapper `parse_slice_null_delimited_double_null_terminated` whenever slice length is
/// known in advance.
///
/// Note that "empty" double-null sequence by itself is a collection of one empty string.
pub unsafe fn parse_ptr_null_delimited_double_null_terminated<'a>(mut p: *const u16) -> Vec<&'a U16CStr> {
    if p.is_null() {
        return vec![];
    }
    let mut strings = Vec::new();
    loop {
        // read one U16CStr
        let string = U16CStr::from_ptr_str(p);
        strings.push(string);
        p = p.add(string.len() + 1);
        // now `p` points at right after NUL which terminates the `string`,
        // which is, either at the beginning of the new str, or at the second terminating NUL.
        if p.read() == u16::NUL {
            break;
        }
    }
    strings
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_double_null_terminated_ptr() {
        unsafe {
            let test_0 = U16String::from_str("\0\0");         // -> ["\0"]
            let test_1 = U16String::from_str("abc\0\0");      // -> ["abc\0"]
            let test_2 = U16String::from_str("\0def\0\0");    // -> ["\0", "def\0"]
            let test_3 = U16String::from_str("abc\0def\0\0"); // -> ["abc\0", "def\0"]
            let cstr_emp = U16CString::from_str_with_nul_unchecked("\0");
            let cstr_abc = U16CString::from_str_with_nul_unchecked("abc\0");
            let cstr_def = U16CString::from_str_with_nul_unchecked("def\0");

            let strings = parse_ptr_null_delimited_double_null_terminated(test_0.as_ptr());
            assert_eq!(strings, vec![cstr_emp.as_ref()]);

            let strings = parse_ptr_null_delimited_double_null_terminated(test_1.as_ptr());
            assert_eq!(strings, vec![cstr_abc.as_ref()]);

            let strings = parse_ptr_null_delimited_double_null_terminated(test_2.as_ptr());
            assert_eq!(strings, vec![cstr_emp.as_ref(), cstr_def.as_ref()]);

            let strings = parse_ptr_null_delimited_double_null_terminated(test_3.as_ptr());
            assert_eq!(strings, vec![cstr_abc.as_ref(), cstr_def.as_ref()]);
        }
    }
}
