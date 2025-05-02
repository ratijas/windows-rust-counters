//! Missing API from `<winnt.h>`

#![allow(bad_style, unused)]

type WORD = u16;
type DWORD = u32;
type LANGID = WORD;
type LCID = DWORD;

#[inline]
pub fn MAKELANGID(p: WORD, s: WORD) -> LANGID {
    (s << 10) | p
}
#[inline]
pub fn PRIMARYLANGID(lgid: LANGID) -> WORD {
    lgid & 0x3ff
}
#[inline]
pub fn SUBLANGID(lgid: LANGID) -> WORD {
    lgid >> 10
}
#[inline]
pub fn MAKELCID(lgid: LANGID, srtid: WORD) -> LCID {
    ((srtid as DWORD) << 16) | (lgid as DWORD)
}
#[inline]
pub fn MAKESORTLCID(lgid: LANGID, srtid: WORD, ver: WORD) -> LCID {
    MAKELCID(lgid, srtid) | ((ver as DWORD) << 20)
}
#[inline]
pub fn LANGIDFROMLCID(lcid: LCID) -> LANGID {
    lcid as LANGID
}
#[inline]
pub fn SORTIDFROMLCID(lcid: LCID) -> WORD {
    ((lcid >> 16) & 0xf) as WORD
}
#[inline]
pub fn SORTVERSIONFROMLCID(lcid: LCID) -> WORD {
    ((lcid >> 16) & 0xf) as WORD
}
