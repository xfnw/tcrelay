use hyper::header::HeaderValue;
use std::ops::RangeInclusive;

pub fn parse(range: &HeaderValue) -> Option<RangeInclusive<usize>> {
    let range = range.as_ref();

    if !range.starts_with(b"bytes=") {
        return None;
    }

    let mut range = range[6..].iter();
    let mut left = 0;
    let mut right = 0;

    for c in &mut range {
        if !c.is_ascii_digit() {
            if c != &b'-' {
                return None;
            }
            break;
        }
        left = left * 10 + (c - b'0') as usize;
    }
    for c in range {
        if !c.is_ascii_digit() {
            if !b", ".contains(c) {
                return None;
            }
            break;
        }
        right = right * 10 + (c - b'0') as usize;
    }

    Some(left..=right)
}
