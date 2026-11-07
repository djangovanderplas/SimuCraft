//! Stable IDs: a kind prefix, `_`, and 12 characters of lowercase Crockford
//! base32 (60 random bits), for example `b_4a10f3k8m2qx`.

/// Lowercase Crockford base32: no i, l, o or u.
pub const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
pub const LEN: usize = 12;

/// Prefixes: model, block, net, annotation.
pub const PREFIXES: [char; 4] = ['m', 'b', 'n', 'a'];

pub fn new(prefix: char) -> String {
    debug_assert!(PREFIXES.contains(&prefix));
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("system random source");
    let mut bits = u64::from_le_bytes(bytes);
    let mut id = format!("{prefix}_");
    for _ in 0..LEN {
        id.push(ALPHABET[(bits & 31) as usize] as char);
        bits >>= 5;
    }
    id
}

pub fn is_valid(id: &str, prefix: char) -> bool {
    let Some(body) = id.strip_prefix(prefix).and_then(|s| s.strip_prefix('_')) else {
        return false;
    };
    body.len() == LEN && body.bytes().all(|c| ALPHABET.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ids_are_valid_and_differ() {
        let a = new('b');
        assert!(is_valid(&a, 'b'), "{a}");
        assert!(!is_valid(&a, 'n'));
        assert_ne!(a, new('b'));
    }

    #[test]
    fn rejects_bad_ids() {
        for bad in [
            "b_a10f33",
            "b_4a10f3k8m2qi",
            "B_4a10f3k8m2qx",
            "b4a10f3k8m2qxz",
            "b_4A10f3k8m2qx",
        ] {
            assert!(!is_valid(bad, 'b'), "{bad}");
        }
    }
}
