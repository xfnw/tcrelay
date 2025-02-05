pub fn hash(state: u16, data: &[u8]) -> u16 {
    let mut state = state;

    for c in data {
        let mut x: u16 = state >> 8 ^ u16::from(*c);
        x ^= x >> 4;
        state = state << 8 ^ x << 12 ^ x << 5 ^ x;
    }

    state
}

macro_rules! make_add {
    ($filter:expr, $data:expr, $($init:expr),*) => {
        $({
            let loc = hash($init, $data) as usize;
            $filter[loc >> 3] |= 1 << (loc & 7);
        })*
    };
}

pub fn add(filter: &mut [u8; 8192], data: &[u8]) {
    make_add!(filter, data, 3, 6, 2, 1);
}

macro_rules! make_check {
    ($filter:expr, $data:expr, $init:expr) => {
        {
            let loc = hash($init, $data) as usize;
            ($filter[loc >> 3] & 1 << (loc & 7)) != 0
        }
    };
    ($filter:expr, $data:expr, $init:expr, $($tail:tt)*) => {
        make_check!($filter, $data, $init) &&
            make_check!($filter, $data, $($tail)*)
    }
}

pub fn check(filter: &[u8; 8192], data: &[u8]) -> bool {
    make_check!(filter, data, 3, 6, 2, 1)
}

#[cfg(test)]
mod tests {
    use crate::bloom::*;

    #[test]
    fn hash_literal() {
        let out = hash(0, b"meow im a fox");
        assert_eq!(out, 29020);
    }

    #[test]
    fn added() {
        let mut filter = [0_u8; 8192];
        add(&mut filter, b"yip");
        add(&mut filter, b"yap");
        add(&mut filter, b"yop");

        assert!(check(&filter, b"yap"));

        // no awoo, $300 fine
        assert!(!check(&filter, b"awoo"));
    }

    #[test]
    fn all_ones() {
        let filter = [255_u8; 8192];

        assert!(check(&filter, b"beep"));
        assert!(check(&filter, b"boop"));
    }
}
