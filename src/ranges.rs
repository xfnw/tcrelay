use hyper::header::HeaderValue;
use std::ops::RangeInclusive;

/// read the value of an HTTP Range header and parse into a range
///
/// len is only needed for a default value when right side of
/// range is unspecified, this function does not check bounds
pub fn parse(range: &HeaderValue, len: usize) -> Option<RangeInclusive<usize>> {
    let range = range.as_ref();

    if !range.starts_with(b"bytes=") {
        return None;
    }

    let mut range = range[6..].iter();
    let mut left: usize = 0;
    let mut right: Option<usize> = None;

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
        right = Some(right.unwrap_or(0) * 10 + (c - b'0') as usize);
    }

    Some(left..=right.unwrap_or(len - 1))
}
