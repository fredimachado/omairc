//! Sidebar networkOrder / collapsedNetworks persistence contracts shared with
//! the TUI (`tui/internal/controller/prefs_test.go`).

use crate::storage::settings::Status;
use crate::util::testdir::TempDir;

use super::persistence::{disk, launch, libera, save, Loopbacks};

fn preferences_line<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}=");
    text.lines().find(|line| line.starts_with(prefix.as_str()))
}

/// Shared fixture with the TUI: sidebar order `zulu, alpha, omarchy`, collapse
/// `alpha` and `omarchy` → `collapsedNetworks=alpha, omarchy`. Also collapses
/// `zulu`+`omarchy` so id-sort (`omarchy, zulu`) cannot pass.
#[test]
fn collapsed_networks_persist_in_sidebar_order() {
    let dir = TempDir::new("collapsed-sidebar-order");
    let path = dir.path().join("omairc.conf");
    let backing = disk(&dir);
    {
        let mut first = launch(&backing, Box::new(Loopbacks::default()));
        save(&mut first, libera("zulu"));
        save(&mut first, libera("alpha"));
        save(&mut first, libera("omarchy"));
        first.set_network_order(&["zulu".to_owned(), "alpha".to_owned(), "omarchy".to_owned()]);
        first.set_network_collapsed("alpha", true);
        first.set_network_collapsed("omarchy", true);
    }
    let text = std::fs::read_to_string(&path).expect("conf");
    assert_eq!(
        preferences_line(&text, "collapsedNetworks"),
        Some("collapsedNetworks=alpha, omarchy"),
        "sidebar-order collapse line (shared with TUI):\n{text}"
    );

    {
        let mut again = launch(&backing, Box::new(Loopbacks::default()));
        again.set_network_collapsed("alpha", false);
        again.set_network_collapsed("zulu", true);
    }
    let text = std::fs::read_to_string(&path).expect("conf");
    assert_eq!(
        preferences_line(&text, "collapsedNetworks"),
        Some("collapsedNetworks=zulu, omarchy"),
        "must follow networkOrder, not id sort:\n{text}"
    );
}

#[test]
fn forget_network_writes_conf_once_and_clears_order_collapse() {
    let dir = TempDir::new("forget-one-write");
    let path = dir.path().join("omairc.conf");
    let backing = disk(&dir);
    {
        let mut first = launch(&backing, Box::new(Loopbacks::default()));
        save(&mut first, libera("zulu"));
        save(&mut first, libera("alpha"));
        save(&mut first, libera("omarchy"));
        first.set_network_order(&["zulu".to_owned(), "alpha".to_owned(), "omarchy".to_owned()]);
        first.set_network_collapsed("alpha", true);
        first.set_network_collapsed("omarchy", true);

        crate::storage::atomicwrite::test_write_count::begin();
        assert_eq!(first.forget_network("alpha"), Status::Written);
        let writes = crate::storage::atomicwrite::test_write_count::end();
        assert_eq!(writes, 1, "forget must write omairc.conf once, got {writes}");
    }
    let text = std::fs::read_to_string(&path).expect("conf");
    assert!(
        !text.contains("[networks/alpha]"),
        "profile group must be gone:\n{text}"
    );
    assert_eq!(
        preferences_line(&text, "networkOrder"),
        Some("networkOrder=zulu, omarchy"),
        "forget must drop id from networkOrder:\n{text}"
    );
    assert_eq!(
        preferences_line(&text, "collapsedNetworks"),
        Some("collapsedNetworks=omarchy"),
        "forget must drop id from collapsedNetworks:\n{text}"
    );
}
