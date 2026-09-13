//! Syntax of an immutable direct child of the Nix store. This validates only
//! identity syntax; privileged callers must separately validate ownership,
//! permissions and current canonical profile targets before activation.
pub fn store_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    (34..=244).contains(&bytes.len())
        && bytes[32] == b'-'
        && bytes[..32]
            .iter()
            .all(|byte| b"0123456789abcdfghijklmnpqrsvwxyz".contains(byte))
        && bytes[33..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+._?=-".contains(byte))
}

pub fn store_basename(value: &str) -> Option<&str> {
    value
        .strip_prefix("/nix/store/")
        .filter(|name| store_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn store_identity_never_accepts_path_components_or_invalid_hashes() {
        let name = "00000000000000000000000000000000-nixos-system";
        assert!(store_name(name));
        assert_eq!(store_basename(&format!("/nix/store/{name}")), Some(name));
        for bad in [
            "",
            "../system",
            "/nix/store/system",
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-system",
            "00000000000000000000000000000000-a/b",
            "00000000000000000000000000000000-a\n",
        ] {
            assert!(!store_name(bad));
        }
    }
}
