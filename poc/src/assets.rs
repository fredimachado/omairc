//! Assets compiled into the binary. The About logo is the same SVG the Qt
//! build loads from `qrc:/icons/omairc.svg`.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

const OMAIRC_SVG: &[u8] = include_bytes!("../../data/icons/hicolor/scalable/apps/omairc.svg");

pub struct OmaircAssets;

impl AssetSource for OmaircAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "icons/omairc.svg" => Some(Cow::Borrowed(OMAIRC_SVG)),
            _ => None,
        })
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
