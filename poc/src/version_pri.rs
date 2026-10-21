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

#[cfg(test)]
mod tests {
    use super::parse_version;

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
}
