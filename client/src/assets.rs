//! Assets compiled into the binary. The About logo is the same SVG the Qt
//! build loads from `qrc:/icons/omairc.svg`. The demo avatars and the network
//! icon are the images Qt's `--demo-server` serves from `qrc:/demo/`, under the
//! same names without the prefix.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

const OMAIRC_SVG: &[u8] = include_bytes!("../../data/icons/hicolor/scalable/apps/omairc.svg");
const MIRA_AVATAR: &[u8] = include_bytes!("../../data/demo/mira-avatar.png");
const ANNA_AVATAR: &[u8] = include_bytes!("../../data/demo/anna-avatar.png");
const KAI_AVATAR: &[u8] = include_bytes!("../../data/demo/kai-avatar.png");
const OMARCHY_ICON: &[u8] = include_bytes!("../../data/demo/omarchy-icon.png");

pub struct OmaircAssets;

impl AssetSource for OmaircAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "icons/omairc.svg" => Some(Cow::Borrowed(OMAIRC_SVG)),
            "demo/mira-avatar.png" => Some(Cow::Borrowed(MIRA_AVATAR)),
            "demo/anna-avatar.png" => Some(Cow::Borrowed(ANNA_AVATAR)),
            "demo/kai-avatar.png" => Some(Cow::Borrowed(KAI_AVATAR)),
            "demo/omarchy-icon.png" => Some(Cow::Borrowed(OMARCHY_ICON)),
            _ => None,
        })
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::controller::Controller;
    use crate::model::presence::bundled_path;

    #[test]
    fn every_seeded_bundled_image_is_embedded() {
        let controller = Controller::seeded_for_tests();
        let mut references: Vec<String> = Vec::new();
        for network_id in controller.network_order() {
            references.extend(controller.features(&network_id).icon_url().map(str::to_owned));
            references.extend(
                controller
                    .nick_facts(&network_id)
                    .values()
                    .filter_map(|facts| facts.avatar.clone()),
            );
        }
        let bundled: Vec<&str> = references
            .iter()
            .filter_map(|reference| bundled_path(reference))
            .collect();
        assert!(!bundled.is_empty());
        for path in bundled {
            let bytes = OmaircAssets.load(path).expect("asset lookup");
            assert!(bytes.is_some(), "{path} is not embedded");
        }
    }
}
