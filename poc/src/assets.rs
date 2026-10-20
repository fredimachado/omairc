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
    use crate::model::load_world;
    use crate::model::presence::bundled_path;

    #[test]
    fn every_seeded_bundled_image_is_embedded() {
        let world = load_world();
        let mut references: Vec<String> = Vec::new();
        for network in &world.networks {
            references.extend(network.icon.clone());
            for member in network.conversations.iter().flat_map(|c| &c.members) {
                references.extend(member.avatar.clone());
            }
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
