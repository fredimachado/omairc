//! The app version: `build.rs` sets `OMAIRC_VERSION` from `OMAIRC_BUILD_VERSION`
//! when packaging/CI pass that env, otherwise from `VERSION` in `version.pri`.
//! `--version`, the About sheet, the sidebar footer, and CTCP VERSION all read
//! it through `version()`, so there is one source. Go twin:
//! `tui/internal/version`.

/// The version `build.rs` baked in at compile time.
pub fn version() -> &'static str {
    env!("OMAIRC_VERSION")
}

#[cfg(test)]
#[path = "version_pri.rs"]
mod version_pri;

#[cfg(test)]
mod tests {
    use super::{
        version,
        version_pri::{parse_version, to_cargo_semver},
    };

    #[test]
    fn compiled_version_is_the_version_pri_value() {
        assert_eq!(Some(version()), parse_version(include_str!("../../version.pri")));
    }

    #[test]
    fn cargo_package_version_matches_version_pri() {
        let pri = parse_version(include_str!("../../version.pri")).expect("version.pri VERSION");
        assert_eq!(env!("CARGO_PKG_VERSION"), to_cargo_semver(pri));
    }
}
