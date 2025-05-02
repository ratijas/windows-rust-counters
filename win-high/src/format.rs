use std::marker::PhantomData;

use crate::prelude::v2::*;

// &'a U16Str -> Iterator<Item=&'a U16CStr>
// &[&U16Str] -> U16CString

/// Iterate oven nul-terminated substrings of the memory buffer which must be
/// terminated by double-nul sequence.
///
/// # Panics
///
/// This function panics if `p` is null.
pub unsafe fn split_nul_delimited_double_nul_terminated_ptr<'a>(
    p: *const u16
) -> NullDelimitedDoubleNullTerminated<'a> {
    assert!(!p.is_null());
    // if buffer consists only of double-nul sequence, move `current` straight to its end.
    let current = if is_double_null_terminator(p) { p.add(1) } else { p };
    // ensure there's an end to this buffer pointed by p.
    // also, move to the second (and the last) nul character of the double-nul terminator.
    let end = find_double_null_terminator(p).add(1);
    NullDelimitedDoubleNullTerminated {
        current,
        end,
        _marker: PhantomData,
    }
}

/// Iterate oven nul-terminated substrings of the slice which must be
/// terminated by double-nul sequence.
///
/// # Panics
///
/// This function panics if slice does not end with double-nul terminator.
pub fn split_nul_delimited_double_nul_terminated<'a>(buf: &U16Str) -> NullDelimitedDoubleNullTerminated<'a> {
    let slice = buf.as_slice();
    assert!(slice.ends_with(&[0u16, 0u16]), "slice must be terminated with double-nul");
    unsafe {
        split_nul_delimited_double_nul_terminated_ptr(slice.as_ptr())
    }
}

/// Join strings with nul and ensure the whole thing is double-nul terminated. Strings must be
/// non-empty to avoid double-nul sequences in the middle. If empty string is encountered, `Err`
/// with its position is returned.
pub fn join_nul_terminate_double_nul(strings: &[&U16CStr]) -> Result<U16String, usize> {
    let mut s = U16String::new();
    for (i, item) in strings.iter().enumerate() {
        if item.is_empty() {
            return Err(i);
        }
        let str = U16Str::from_slice(U16CStr::as_slice_with_nul(item));
        s.push(str);
    }
    // ensure string is double-nul terminated
    if s.is_empty() {
        s.push_slice(&[0u16, 0u16]);
    } else {
        // one nul is already there as a part of the last string's nul terminator
        s.push_slice(&[0u16]);
    }
    Ok(s)
}

pub struct NullDelimitedDoubleNullTerminated<'a> {
    /// Pointer to the start of the next substring, or at the last nul character of the double-nul
    /// terminator.
    current: *const u16,
    /// Pointer to the last nul character of the double-nul terminator.
    end: *const u16,
    _marker: PhantomData<&'a ()>,
}

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
pub unsafe fn find_double_null_terminator(mut p: *const u16) -> *const u16 {
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
pub unsafe fn is_double_null_terminator(p: *const u16) -> bool {
    assert!(!p.is_null());

    p.read() == 0 && p.add(1).read() == 0
}

mod imp {
    use super::*;

    impl<'a> Iterator for NullDelimitedDoubleNullTerminated<'a> {
        type Item = &'a U16CStr;

        fn next(&mut self) -> Option<Self::Item> {
            debug_assert!(!self.current.is_null() && !self.end.is_null());
            if self.current == self.end {
                None
            } else {
                // SAFETY: self.current is not pointing at nul, but nul terminator is
                // guaranteed to exist and is readable, so it is safe to construct U16CStr from it.
                unsafe {
                    let uc_str = U16CStr::from_ptr_str(self.current);
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

#[cfg(test)]
mod test {
    use super::*;
    use itertools::Itertools;

    const U_EMP: &'static U16Str = u16str!("");                 // panic!()
    const U_NUL_NUL: &'static U16Str = u16str!("\0\0");         // []
    const U_ABC: &'static U16Str = u16str!("abc\0\0");          // ["abc\0"]
    const U_ABC_DEF: &'static U16Str = u16str!("abc\0def\0\0"); // ["abc\0", "def\0"]
    const U_NUL_DEF: &'static U16Str = u16str!("\0def\0\0");    // ["\0", "def\0"]

    const UC_EMP: &'static U16CStr = u16cstr!("");
    const UC_ABC: &'static U16CStr = u16cstr!("abc");
    const UC_DEF: &'static U16CStr = u16cstr!("def");

    #[test]
    fn test_find_double_null_terminator_empty() {
        unsafe {
            let end = find_double_null_terminator(U_NUL_NUL.as_ptr());
            assert_eq!(U_NUL_NUL.as_ptr(), end);
        }
    }

    #[test]
    fn test_find_double_null_terminator_single() {
        unsafe {
            let end = find_double_null_terminator(U_ABC.as_ptr());
            assert_eq!(U_ABC.as_ptr().add("abc".len()), end);
        }
    }

    #[test]
    fn test_find_double_null_terminator_multi() {
        unsafe {
            let end = find_double_null_terminator(U_ABC_DEF.as_ptr());
            assert_eq!(U_ABC_DEF.as_ptr().add("abc\0def".len()), end);
        }
    }

    #[test]
    fn test_find_double_null_terminator_starts_with_single_nul() {
        unsafe {
            let end = find_double_null_terminator(U_NUL_DEF.as_ptr());
            assert_eq!(U_NUL_DEF.as_ptr().add("\0def".len()), end);
        }
    }

    #[test]
    fn test_split_ptr_empty() {
        unsafe {
            let split = split_nul_delimited_double_nul_terminated_ptr(U_NUL_NUL.as_ptr());
            assert!(split.collect_vec().is_empty());
        }
    }

    #[test]
    fn test_split_ptr_single() {
        unsafe {
            let split = split_nul_delimited_double_nul_terminated_ptr(U_ABC.as_ptr());
            assert_eq!(split.collect_vec(), &[UC_ABC]);
        }
    }

    #[test]
    fn test_split_ptr_multi() {
        unsafe {
            let split = split_nul_delimited_double_nul_terminated_ptr(U_ABC_DEF.as_ptr());
            assert_eq!(split.collect_vec(), &[UC_ABC, UC_DEF]);
        }
    }

    #[test]
    fn test_split_ptr_starts_with_single_nul() {
        unsafe {
            let split = split_nul_delimited_double_nul_terminated_ptr(U_NUL_DEF.as_ptr());
            assert_eq!(split.collect_vec(), &[UC_EMP, UC_DEF]);
        }
    }

    #[test]
    #[should_panic(expected = "slice must be terminated with double-nul")]
    fn test_split_slice_no_data() {
        let split = split_nul_delimited_double_nul_terminated(&*U_EMP);
        let _ = split.collect_vec();
    }

    #[test]
    fn test_split_slice_empty() {
        let split = split_nul_delimited_double_nul_terminated(&*U_NUL_NUL);
        assert!(split.collect_vec().is_empty());
    }

    #[test]
    fn test_split_slice_single() {
        let split = split_nul_delimited_double_nul_terminated(&*U_ABC);
        assert_eq!(split.collect_vec(), &[UC_ABC]);
    }

    #[test]
    fn test_split_slice_multi() {
        unsafe {
            let split = split_nul_delimited_double_nul_terminated_ptr(U_ABC_DEF.as_ptr());
            assert_eq!(split.collect_vec(), &[UC_ABC, UC_DEF]);
        }
    }

    #[test]
    fn test_split_slice_starts_with_single_nul() {
        unsafe {
            let split = split_nul_delimited_double_nul_terminated_ptr(U_NUL_DEF.as_ptr());
            assert_eq!(split.collect_vec(), &[UC_EMP, UC_DEF]);
        }
    }

    #[test]
    fn test_join_empty() {
        let buf = join_nul_terminate_double_nul(&[]).unwrap();
        assert_eq!(buf, U_NUL_NUL);
    }

    #[test]
    fn test_join_single() {
        let buf = join_nul_terminate_double_nul(&[UC_ABC]).unwrap();
        assert_eq!(buf, U_ABC);
    }

    #[test]
    fn test_join_multi() {
        let buf = join_nul_terminate_double_nul(&[UC_ABC, UC_DEF]).unwrap();
        assert_eq!(buf, U_ABC_DEF);
    }

    #[test]
    fn test_join_has_empty_cstr() {
        let res = join_nul_terminate_double_nul(&[UC_ABC, UC_EMP, UC_DEF]);
        assert_eq!(res, Err(1));
    }
}
