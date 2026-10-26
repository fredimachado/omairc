//! Reads the `VERSION = x.y.z` value from `version.pri`, the repository's single
//! version source. `build.rs` compiles this file to set `OMAIRC_VERSION`, and the
//! app compiles it under `cfg(test)` to check the compiled version against the file.
//! Qt reads the same value through `bin/gen-omaircversion-h.py`.

/// The `VERSION` value of a `version.pri` body, or `None` when it is missing or empty.
pub fn parse_version(pri: &str) -> Option<&str> {
    pri.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("VERSION")?;
        let value = rest.trim_start().strip_prefix('=')?.trim();
        (!value.is_empty()).then_some(value)
    })
}

/// Arch pkgver cannot contain hyphens, so letter suffixes sit flush
/// (`1.0.7alpha`). Cargo SemVer needs a pre-release separator (`1.0.7-alpha`).
pub fn to_cargo_semver(version: &str) -> String {
    let bytes = version.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            i += 1;
            continue;
        }
        if b == b'.' {
            i += 1;
            continue;
        }
        break;
    }
    if i > 0
        && i < bytes.len()
        && bytes[i].is_ascii_alphabetic()
        && bytes[i - 1].is_ascii_digit()
    {
        format!("{}-{}", &version[..i], &version[i..])
    } else {
        version.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_version, to_cargo_semver};

    #[test]
    fn version_comes_from_the_version_pri_value() {
        assert_eq!(parse_version("# note\nVERSION = 1.10.3\nOTHER = x\n"), Some("1.10.3"));
        assert_eq!(parse_version("VERSION=2.0.0"), Some("2.0.0"));
        assert_eq!(parse_version("VERSION = \n"), None);
        assert_eq!(parse_version("OMAIRC_VERSION_HEADER = x"), None);
    }

    #[test]
    fn the_shipped_version_pri_parses() {
        assert!(parse_version(include_str!("../../version.pri")).is_some());
    }

    #[test]
    fn letter_suffix_becomes_semver_prerelease() {
        assert_eq!(to_cargo_semver("1.0.7alpha"), "1.0.7-alpha");
        assert_eq!(to_cargo_semver("1.10.3"), "1.10.3");
        assert_eq!(to_cargo_semver("2.0.0beta1"), "2.0.0-beta1");
        assert_eq!(to_cargo_semver("1.0.7rc2"), "1.0.7-rc2");
    }
}
