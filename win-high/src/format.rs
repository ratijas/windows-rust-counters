use std::marker::PhantomData;

use crate::prelude::v1::*;

// &'a UStr<Char> -> Iterator<Item=&'a UCStr<Char>>
// &[&UStr<Char>] -> UCString<Char>

/// Iterate oven nul-terminated substrings of the memory buffer which
/// is terminated by double-nul sequence.
///
/// # Panics
///
/// This function panics if `p` is null.
pub unsafe fn split_null_delimited_double_null_terminated_ptr<'a, C: UChar + 'static>(
    p: *const C
) -> impl Iterator<Item=&'a UCStr<C>> {
    assert!(!p.is_null());
    // if buffer consists only of double-nul sequence, move `current` straight to its end.
    let current = if is_double_null_terminator(p) { p.add(1) } else { p };
    // ensure there's an end to this buffer pointed by p.
    // also, move to the second (and the last) nul character of the double-nul terminator.
    let end = find_double_null_terminator(p).add(1);
    NullDelimitedDoubleNullTerminatedPtr {
        current,
        end,
        _marker: PhantomData,
    }
}

struct NullDelimitedDoubleNullTerminatedPtr<'a, C: UChar> {
    /// Pointer to the start of the next substring, or at the last nul character of the double-nul
    /// terminator.
    current: *const C,
    /// Pointer to the last nul character of the double-nul terminator.
    end: *const C,
    _marker: PhantomData<&'a C>,
}

// pub fn split_slice<C: UChar + 'static>(slice: &UStr<C>) -> impl Iterator<Item=&UCStr<C>> {
//     NullDelimitedDoubleNullTerminatedSlice { inner: slice }
// }

// struct NullDelimitedDoubleNullTerminatedSlice<'a, C: UChar> {
//     inner: &'a UStr<C>,
// }

/// Read and increment the pointer until double-NULL terminator is found.
/// Doing so, eventually would either return a valid pointer, or segfault.
///
/// # Returns
///
/// Returns a pointer to the double-NUL terminator sequence.
///
/// # Panics
///
/// This function panics if `p` is null.
pub unsafe fn find_double_null_terminator<C: UChar>(mut p: *const C) -> *const C {
    assert!(!p.is_null());

    while !is_double_null_terminator(p) {
        p = p.add(1);
    }
    p
}

/// Reads item pointed by this `p` and the next item right after it,
/// and compare both items with NULL value for their type.
///
/// # Panics
///
/// This function panics if `p` is null.
pub unsafe fn is_double_null_terminator<C: UChar>(p: *const C) -> bool {
    assert!(!p.is_null());

    p.read() == C::NUL && p.add(1).read() == C::NUL
}

mod imp {
    use super::*;

    impl<'a, C: UChar> Iterator for NullDelimitedDoubleNullTerminatedPtr<'a, C> {
        type Item = &'a UCStr<C>;

        fn next(&mut self) -> Option<Self::Item> {
            debug_assert!(!self.current.is_null() && !self.end.is_null());
            if self.current == self.end {
                None
            } else {
                // SAFETY: self.current is not pointing at nul, but nul terminator is
                // guaranteed to exist and is readable, so it is safe to construct UCStr from it.
                unsafe {
                    let uc_str = UCStr::from_ptr_str(self.current);
                    // points at the next string after the nul terminator of the uc_str,
                    // or at the self.end, because after last string and its nul terminator comes
                    // the second nul terminator which is the end of the double-nul terminator
                    // as well.
                    self.current = self.current.add(uc_str.len() + 1);
                    debug_assert!(self.current <= self.end);
                    Some(uc_str)
                }
            }
        }
    }
}

/// Commonly used raw byte string format uses 0u16 as a delimiter for UTF-16 strings
/// terminated by double 0u16 sequence.
pub fn parse_slice_null_delimited_double_null_terminated(input: &[u16]) -> Vec<&U16CStr> {
    if !input.ends_with(&[u16::NUL; 2]) {
        return vec![];
    }
    unsafe { parse_ptr_null_delimited_double_null_terminated(input.as_ptr()) }
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
    use itertools::Itertools;

    #[test]
    fn test_find_double_null_terminator_empty() {
        unsafe {
            let buf = U16String::from_str("\0\0");
            let end = find_double_null_terminator(buf.as_ptr());
            assert_eq!(buf.as_ptr(), end);
        }
    }

    #[test]
    fn test_find_double_null_terminator_single() {
        unsafe {
            let buf = U16String::from_str("abc\0\0");
            let end = find_double_null_terminator(buf.as_ptr());
            assert_eq!(buf.as_ptr().add("abc".len()), end);
        }
    }

    #[test]
    fn test_find_double_null_terminator_multi() {
        unsafe {
            let buf = U16String::from_str("abc\0def\0\0");
            let end = find_double_null_terminator(buf.as_ptr());
            assert_eq!(buf.as_ptr().add("abc\0def".len()), end);
        }
    }

    #[test]
    fn test_split_empty() {
        unsafe {
            let test = U16String::from_str("\0\0");         // -> ["\0"]
            let split = split_null_delimited_double_null_terminated_ptr(test.as_ptr());
            assert!(split.collect_vec().is_empty());
        }
    }

    #[test]
    fn test_split_single() {
        unsafe {
            let test = U16String::from_str("abc\0\0");      // -> ["abc\0"]
            let cstr_abc = U16CString::from_str_with_nul_unchecked("abc\0");
            let split = split_null_delimited_double_null_terminated_ptr(test.as_ptr());
            assert_eq!(split.collect_vec(), vec![cstr_abc.as_ref()]);
        }
    }

    #[test]
    fn test_split_multi() {
        unsafe {
            let test = U16String::from_str("abc\0def\0\0"); // -> ["abc\0", "def\0"]
            let cstr_abc = U16CString::from_str_with_nul_unchecked("abc\0");
            let cstr_def = U16CString::from_str_with_nul_unchecked("def\0");
            let split = split_null_delimited_double_null_terminated_ptr(test.as_ptr());
            assert_eq!(split.collect_vec(), vec![cstr_abc.as_ref(), cstr_def.as_ref()]);
        }
    }

    #[test]
    fn test_split_starts_with_single_nul() {
        unsafe {
            let test = U16String::from_str("\0def\0\0");    // -> ["\0", "def\0"]
            let cstr_emp = U16CString::from_str_with_nul_unchecked("\0");
            let cstr_def = U16CString::from_str_with_nul_unchecked("def\0");
            let split = split_null_delimited_double_null_terminated_ptr(test.as_ptr());
            assert_eq!(split.collect_vec(), vec![cstr_emp.as_ref(), cstr_def.as_ref()]);
        }
    }
}
