//! Example for the issue at nom repository on GitHub:
//! https://github.com/Geal/nom/issues/1122
#[macro_use]
extern crate nom;
use std::mem;

// functional style
pub fn take_struct<S>(input: &[u8]) -> nom::IResult<&[u8], &S> {
    // SAFETY: `take` ensures there is enough bytes in `s` slice to view it as an `S`.
    nom::combinator::map(
        nom::bytes::complete::take(mem::size_of::<S>()),
        |s: &[u8]| unsafe { (s.as_ptr() as *const S).as_ref().unwrap() }
    )(input)
}
// macro style
named!(protocol<&MyFfi>,
    preceded!(
        tag!(&b"prefix_"[..]),
        call!(take_struct::<MyFfi>)
    )
);
#[repr(C)]
#[derive(Debug, Eq, PartialEq)]
pub struct MyFfi {
    pub number: u32,
    pub payload: [u8; 4],
}
fn main() {
    let a = b"prefix_\x2a\0\0\01234_suffix";
    let res = MyFfi {
        number: 42,
        payload: ['1' as u8, '2' as u8, '3' as u8, '4' as u8],
    };
    assert_eq!(protocol(&a[..]), Ok((&b"_suffix"[..], &res)));
}
