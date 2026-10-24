//! The app version: `build.rs` sets `OMAIRC_VERSION` from `OMAIRC_BUILD_VERSION`
//! when packaging/CI pass that env, otherwise from `VERSION` in `version.pri`.
//! `--version`, the About sheet, the sidebar footer and a future CTCP VERSION
//! reply all read it through `version()`, so there is one source. Go twin:
//! `tui/internal/version`. C++ twin: `omaircversion.h`, which
//! `bin/gen-omaircversion-h.py` generates from `version.pri`.

/// The version `build.rs` baked in at compile time.
pub fn version() -> &'static str {
    env!("OMAIRC_VERSION")
}

#[cfg(test)]
#[path = "version_pri.rs"]
mod version_pri;

#[cfg(test)]
mod tests {
    use super::{version, version_pri::parse_version};

    #[test]
    fn compiled_version_is_the_version_pri_value() {
        assert_eq!(Some(version()), parse_version(include_str!("../../version.pri")));
    }
}
