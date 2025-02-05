use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{body::Bytes, header::HeaderValue, Response};
use std::{cmp::min, ops::RangeInclusive};

/// read the value of an HTTP Range header and parse into a range
///
/// will not return an empty or out of bounds range, should be
/// fine to pass to Bytes.slice after unwrapping
pub fn parse(range: &HeaderValue, len: usize) -> Option<RangeInclusive<usize>> {
    if len == 0 {
        return None;
    }

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

    let Some(range) = parse(range, olength) else {
        return not_satisfiable();
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

#[cfg(test)]
mod tests {
    use crate::ranges::*;
    use hyper::body::Body;

    macro_rules! assert_parse {
        ($inp:expr, $len:expr, $expect:expr) => {
            assert_eq!(parse(&HeaderValue::from_static($inp), $len), $expect);
        };
        ($(($inp:expr, $len:expr, $expect:expr)),*) => {$(
            assert_parse!($inp, $len, Some($expect));
        )*};
        ($(($inp:expr, $len:expr)),*) => {$(
            assert_parse!($inp, $len, None);
        )*};
    }

    #[test]
    fn valid_ranges() {
        assert_parse!(
            ("bytes=0-0", 1, 0..=0),
            ("bytes=0-", 43, 0..=42),
            ("bytes=5-9", 10, 5..=9),
            ("bytes=5-10", 10, 5..=9),
            ("bytes=5-90", 10, 5..=9),
            ("bytes=5-90, 4-5", 10, 5..=9),
            ("bytes=5-90 ", 10, 5..=9),
            ("bytes=555555-", 1000000, 555555..=999999)
        );
    }

    #[test]
    fn invalid_ranges() {
        assert_parse!(
            ("bytes=0-0", 0),
            ("bytes=69-", 42),
            ("bytes=69-420", 31),
            ("bytes=420-69", 621),
            ("bytes=3-3.14", 3621),
            ("boots=5-9", 10),
            ("bytes==5-9", 10)
        );
    }

    #[test]
    fn valid_request() {
        let range = HeaderValue::from_static("bytes=3-5");
        let data = Bytes::from_static(b"beep boop");
        let res = Response::builder().header("Accept-Ranges", "bytes");
        let res = ranged_response(res, data, &range).unwrap();
        assert_eq!(res.status(), 206);
        assert_eq!(
            res.headers().get("Content-Range"),
            Some(&HeaderValue::from_static("3-5/9"))
        );
        assert_eq!(res.body().size_hint().exact(), Some(3));
    }

    #[test]
    fn invalid_request() {
        let range = HeaderValue::from_static("bytes=meow");
        let data = Bytes::from_static(b"beep boop");
        let res = Response::builder().header("Accept-Ranges", "bytes");
        let res = ranged_response(res, data, &range);
        assert_eq!(res.unwrap().status(), 416);
    }
}
