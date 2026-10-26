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
///
/// Same algorithm as `bin/check-conventions` `version_pri_to_cargo_semver`:
/// match `^(\d+(?:\.\d+)*)([A-Za-z].*)$` and insert `-` between the groups;
/// otherwise return the input unchanged.
pub fn to_cargo_semver(version: &str) -> String {
    let bytes = version.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() {
        return version.to_string();
    }

    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    loop {
        if i >= bytes.len() || bytes[i] != b'.' {
            break;
        }
        let mut j = i + 1;
        if j >= bytes.len() || !bytes[j].is_ascii_digit() {
            break;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        i = j;
    }

    if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
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

    // Keep in lockstep with bin/check-conventions VERSION_PRI_TO_CARGO_CASES.
    #[test]
    fn letter_suffix_becomes_semver_prerelease() {
        let cases = [
            ("1.0.7alpha", "1.0.7-alpha"),
            ("1.10.3", "1.10.3"),
            ("2.0.0beta1", "2.0.0-beta1"),
            ("1.0.7rc2", "1.0.7-rc2"),
            ("1alpha", "1-alpha"),
            ("1.0a2", "1.0-a2"),
            ("1.2.3.4rc1", "1.2.3.4-rc1"),
            ("1..2alpha", "1..2alpha"),
            ("..1alpha", "..1alpha"),
            ("1.0.7.alpha", "1.0.7.alpha"),
            ("1.0.7-alpha", "1.0.7-alpha"),
            ("1.0.7+build", "1.0.7+build"),
        ];
        for (raw, want) in cases {
            assert_eq!(to_cargo_semver(raw), want, "raw={raw:?}");
        }
    }
}
