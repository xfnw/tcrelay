use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{body::Bytes, header::HeaderValue, Response};
use std::{cmp::min, ops::RangeInclusive};

/// read the value of an HTTP Range header and parse into a range
///
/// will not return an empty or out of bounds range, should be
/// fine to pass to Bytes.slice after unwrapping
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

    // http ranges are inclusive
    let len = len - 1;

    let right = min(right.unwrap_or(len), len);
    if left > right {
        return None;
    }

    Some(left..=right)
}

fn not_satisfiable() -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::http::Error> {
    Response::builder()
        .status(hyper::StatusCode::RANGE_NOT_SATISFIABLE)
        .body(
            Full::new(Bytes::from_static(b"U WOT M8\n"))
                .map_err(|e| match e {})
                .boxed(),
        )
}

pub fn ranged_response(
    res: http::response::Builder,
    data: Bytes,
    range: &HeaderValue,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::http::Error> {
    let olength = data.len();

    let range = match parse(range, olength) {
        Some(r) => r,
        None => return not_satisfiable(),
    };

    let res = res
        .header(
            "Content-Range",
            format!("{}-{}/{}", range.start(), range.end(), olength),
        )
        .status(hyper::StatusCode::PARTIAL_CONTENT);
    let data = data.slice(range);

    res.body(Full::new(data).map_err(|e| match e {}).boxed())
}
