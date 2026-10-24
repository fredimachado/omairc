//! The ownership rules for the Windows toast records, as plain functions with no Windows calls.
//! `windows.rs` reads the registry, the Start Menu links, and the install folders, then asks
//! these functions what this build may do. The exe paths it compares are canonical (see
//! `windows::canonical_exe`), so `util::same_exe` compares like with like. The rules themselves
//! are stated in the module doc of `windows.rs`.

use std::path::{Component, Path, PathBuf};

use super::util::{same_exe, should_claim};

/// The path a comparison uses: `resolve`'s canonical form when the disk gives one, with the
/// `\\?\` verbatim prefix removed, and the path as given when it gives none.
pub fn canonical_with(path: &Path, resolve: impl Fn(&Path) -> Option<PathBuf>) -> PathBuf {
    strip_verbatim(&resolve(path).unwrap_or_else(|| path.to_path_buf()))
}

/// Drops the `\\?\` verbatim prefix that `canonicalize` adds on Windows, so `\\?\C:\x` and
/// `C:\x` compare equal. `\\?\UNC\server\share` becomes `\\server\share`.
fn strip_verbatim(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(plain) = text.strip_prefix(r"\\?\") {
        PathBuf::from(plain)
    } else {
        path.to_path_buf()
    }
}

/// Whether `this_exe` lies inside one of `install_roots`. `resolve` names the path the disk
/// gives, or `None` when there is none, and a path with no resolution is compared as given. The
/// exe and each root are resolved, then stripped of the verbatim prefix, before the comparison.
/// Path components are compared one at a time, ignoring ASCII case, so `...\Omairc-dev\x.exe`
/// is not inside `...\Omairc`. A `..` left in the compared exe path is refused, since it could
/// leave the root without changing the prefix.
pub fn is_installed_exe(
    this_exe: &Path,
    install_roots: &[PathBuf],
    resolve: impl Fn(&Path) -> Option<PathBuf>,
) -> bool {
    let resolved = |path: &Path| canonical_with(path, &resolve);
    let exe = resolved(this_exe);
    let exe_parts: Vec<Component<'_>> = exe.components().collect();
    if exe_parts.contains(&Component::ParentDir) {
        return false;
    }
    install_roots.iter().any(|root| {
        let root = resolved(root);
        let root_parts: Vec<Component<'_>> = root.components().collect();
        !root_parts.is_empty()
            && root_parts.len() < exe_parts.len()
            && root_parts.iter().zip(&exe_parts).all(|(root_part, exe_part)| {
                root_part.as_os_str().eq_ignore_ascii_case(exe_part.as_os_str())
            })
    })
}

/// Where a Start Menu `Omairc.lnk` lives. Only `PerUser` is ever written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkSlot {
    AllUsers,
    PerUser,
}

/// One Start Menu `Omairc.lnk`, as read from disk. Only a confirmed absence is `Missing`. A
/// `Targets` path is in the canonical form `canonical_with` gives.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkState {
    Missing,
    /// The link exists, but its target cannot be read. Fail closed.
    Unreadable,
    /// The link names an exe path.
    Targets(PathBuf),
}

/// Why the shortcut fallback must not write the per-user link.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkRefusal {
    Unreadable(LinkSlot),
    NamesOtherExe(LinkSlot, PathBuf),
}

/// Decides whether the shortcut fallback may write the per-user link, given both links as
/// read from disk. An unreadable link refuses. Any other link allows when this exe is an
/// installed copy, or when `should_claim` allows it: the link names this exe, or an exe that is
/// gone. The all-users link is checked first and the first refusal wins, so when both links
/// refuse, the all-users refusal is returned. `this_exe` and each `Targets` path are canonical.
pub fn check_existing_links(
    this_exe: &Path,
    install_roots: &[PathBuf],
    all_users: &LinkState,
    per_user: &LinkState,
    exists: impl Fn(&Path) -> bool,
    resolve: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<(), LinkRefusal> {
    let from_install = is_installed_exe(this_exe, install_roots, resolve);
    for (slot, state) in [(LinkSlot::AllUsers, all_users), (LinkSlot::PerUser, per_user)] {
        match state {
            LinkState::Missing => {}
            LinkState::Unreadable => return Err(LinkRefusal::Unreadable(slot)),
            LinkState::Targets(target) => {
                if !from_install && !should_claim(Some(target.as_path()), this_exe, &exists) {
                    return Err(LinkRefusal::NamesOtherExe(slot, target.clone()));
                }
            }
        }
    }
    Ok(())
}

/// Decides what a link's raw target means. `raw` is what `shortcut_target` read, or `None` when
/// that read failed. `expand` turns `%VAR%` text into a path, and returns `None` when it fails.
/// The link is `Unreadable` when the read or the expansion fails, when the result is empty
/// (an empty target names no exe), or when `%` survives expansion. A variable this session cannot
/// resolve leaves its text in place, and that text is not a path to test against the disk.
pub fn link_state_from_target(
    raw: Option<&str>,
    expand: impl Fn(&str) -> Option<String>,
) -> LinkState {
    match raw.and_then(expand) {
        Some(path) if !path.is_empty() && !path.contains('%') => {
            LinkState::Targets(PathBuf::from(path))
        }
        _ => LinkState::Unreadable,
    }
}

/// Decides what the repair reports for the links that name this exe. `is_stamped` says whether a
/// link already carries the stamp, and `stamp` writes it and says whether the write took. When a
/// link is stamped, nothing is written. Otherwise `stamp` is tried on each link in order, and the
/// first that takes ends the attempts. The result is whether `installed` has any link, whatever
/// the stamps did, so a link found by name counts even when no stamp takes.
pub fn found_after_repair(
    installed: &[PathBuf],
    is_stamped: impl Fn(&Path) -> bool,
    stamp: impl Fn(&Path) -> bool,
) -> bool {
    if !installed.iter().any(|link| is_stamped(link)) {
        for link in installed {
            if stamp(link) {
                break;
            }
        }
    }
    !installed.is_empty()
}

/// Whether `ensure_notification_registration` stops after the repair, before the fallback runs.
/// `repaired` is true when a readable link in either location names this exe, whether or not it
/// is stamped (see `found_after_repair`). When it is false, nothing stops here and the fallback
/// runs as it always has. When it is true, a build that is not installed stops, portable builds
/// included, so it does not overwrite a link it did not own. An installed copy goes on only when
/// its per-user link is readable and names another exe, which it may replace. A missing per-user
/// link, an unreadable one, or one that names this exe stops, so after a repair a per-user link is
/// replaced, never created.
pub fn repair_ends_registration(
    repaired: bool,
    from_install: bool,
    this_exe: &Path,
    per_user: &LinkState,
) -> bool {
    if !repaired {
        return false;
    }
    if !from_install {
        return true;
    }
    match per_user {
        LinkState::Targets(target) => same_exe(target, this_exe),
        LinkState::Missing | LinkState::Unreadable => true,
    }
}

/// Whether this build may point the toast activator at `this_exe`. `existing` is the exe the
/// registration names, or `None` when nothing is registered. An installed copy may always, even
/// when `existing` still exists. Any other build may only when `should_claim` allows it.
pub fn activator_claim(
    this_exe: &Path,
    install_roots: &[PathBuf],
    existing: Option<&Path>,
    resolve: impl Fn(&Path) -> Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> bool {
    is_installed_exe(this_exe, install_roots, resolve) || should_claim(existing, this_exe, exists)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    const THIS_EXE: &str = r"C:\dev\omairc\omairc-poc-gpui.exe";
    const INSTALLED_EXE: &str = r"C:\Program Files\Omairc\omairc.exe";
    const OTHER_EXE: &str = r"C:\Program Files\Other\omairc.exe";
    /// A dev exe in another checkout. No test puts it on disk.
    const DEV_EXE: &str = r"D:\other-checkout\poc\target\debug\omairc-poc-gpui.exe";
    const PER_USER_ROOT: &str = r"C:\Users\Fredi\AppData\Local\Programs\Omairc";
    const ALL_USERS_ROOT: &str = r"C:\Program Files\Omairc";
    const ROOT_TARGET: &str = r"%ROOT%\omairc.exe";

    /// Stands in for the disk where a test needs no resolution: nothing has a canonical form.
    fn no_resolve(_: &Path) -> Option<PathBuf> {
        None
    }

    /// Stands in for `canonicalize` on a machine where `C:\PROGRA~1` names `C:\Program Files`.
    fn expand_8_3(path: &Path) -> Option<PathBuf> {
        let rest = path.to_str()?.strip_prefix(r"C:\PROGRA~1\")?;
        Some(PathBuf::from(format!(r"C:\Program Files\{rest}")))
    }

    /// Stands in for `canonicalize` on the exe only: it adds the verbatim prefix it returns on
    /// Windows, and the roots resolve to nothing, so they stay plain.
    fn verbatim_exe_only(path: &Path) -> Option<PathBuf> {
        let is_exe = path.extension().and_then(|ext| ext.to_str()) == Some("exe");
        is_exe.then(|| PathBuf::from(format!(r"\\?\{}", path.display())))
    }

    fn test_roots() -> Vec<PathBuf> {
        vec![PathBuf::from(PER_USER_ROOT), PathBuf::from(ALL_USERS_ROOT)]
    }

    #[test]
    fn an_exe_under_the_per_user_root_is_installed() {
        let exe = Path::new(r"C:\Users\Fredi\AppData\Local\Programs\Omairc\omairc.exe");
        assert!(is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn an_exe_under_the_all_users_root_is_installed() {
        assert!(is_installed_exe(Path::new(INSTALLED_EXE), &test_roots(), no_resolve));
    }

    #[test]
    fn a_dev_build_in_the_checkout_is_not_installed() {
        let exe = Path::new(r"D:\code\omairc\poc\target\debug\omairc-poc-gpui.exe");
        assert!(!is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn a_lookalike_root_is_not_installed() {
        let exe = Path::new(r"C:\Users\Fredi\AppData\Local\Programs\Omairc-dev\x.exe");
        assert!(!is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn root_matching_ignores_ascii_case() {
        let exe = Path::new(r"c:\users\fredi\appdata\local\programs\OMAIRC\omairc.exe");
        assert!(is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn a_verbatim_exe_path_matches_the_plain_root() {
        let exe = Path::new(r"\\?\C:\Users\Fredi\AppData\Local\Programs\Omairc\omairc.exe");
        assert!(is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn a_verbatim_root_matches_the_plain_exe_path() {
        let roots = [PathBuf::from(r"\\?\C:\Program Files\Omairc")];
        assert!(is_installed_exe(Path::new(INSTALLED_EXE), &roots, no_resolve));
    }

    #[test]
    fn a_resolved_verbatim_path_matches_the_plain_root() {
        // Only the exe resolves to the verbatim form. The root stays plain, so the strip must
        // run on the resolved exe for the two to compare equal.
        let exe = Path::new(INSTALLED_EXE);
        assert!(is_installed_exe(exe, &test_roots(), verbatim_exe_only));
    }

    #[test]
    fn an_8_3_exe_path_matches_its_long_path_through_resolution() {
        let short = Path::new(r"C:\PROGRA~1\Omairc\omairc.exe");
        assert!(is_installed_exe(short, &test_roots(), expand_8_3));
        assert!(
            !is_installed_exe(short, &test_roots(), no_resolve),
            "without resolution the short name is refused, which fails safe"
        );
    }

    #[test]
    fn a_short_name_names_its_long_path_once_resolved() {
        let resolved = canonical_with(Path::new(r"C:\PROGRA~1\Omairc\omairc.exe"), expand_8_3);
        assert_eq!(resolved, PathBuf::from(INSTALLED_EXE));
        assert!(same_exe(&resolved, Path::new(INSTALLED_EXE)));
    }

    #[test]
    fn a_unc_verbatim_path_strips_to_a_unc_path() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\server\share\omairc.exe")),
            PathBuf::from(r"\\server\share\omairc.exe"),
        );
    }

    #[test]
    fn a_parent_dir_cannot_leave_the_root() {
        let exe = Path::new(r"C:\Users\Fredi\AppData\Local\Programs\Omairc\..\..\dev\omairc.exe");
        assert!(!is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn no_roots_or_an_empty_root_matches_nothing() {
        let exe = Path::new(THIS_EXE);
        assert!(!is_installed_exe(exe, &[], no_resolve));
        assert!(!is_installed_exe(exe, &[PathBuf::new()], no_resolve));
    }

    #[test]
    fn an_installed_copy_takes_the_activator_from_a_still_existing_exe() {
        let installed = Path::new(INSTALLED_EXE);
        let other = Path::new(OTHER_EXE);
        assert!(activator_claim(
            installed,
            &test_roots(),
            Some(other),
            no_resolve,
            |path| path == other,
        ));
    }

    #[test]
    fn a_dev_build_keeps_the_activator_from_a_still_existing_exe() {
        let installed = Path::new(INSTALLED_EXE);
        assert!(!activator_claim(
            Path::new(THIS_EXE),
            &test_roots(),
            Some(installed),
            no_resolve,
            |path| path == installed,
        ));
    }

    #[test]
    fn a_dev_build_takes_the_activator_when_its_owner_is_gone() {
        let gone = Path::new(r"C:\old\omairc\omairc.exe");
        assert!(activator_claim(
            Path::new(THIS_EXE),
            &test_roots(),
            Some(gone),
            no_resolve,
            |_| false,
        ));
    }

    #[test]
    fn a_dev_build_takes_an_unregistered_activator() {
        assert!(activator_claim(
            Path::new(THIS_EXE),
            &test_roots(),
            None,
            no_resolve,
            |_| false,
        ));
    }

    #[test]
    fn with_no_links_the_fallback_may_write() {
        let this_exe = Path::new(THIS_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &LinkState::Missing,
                &LinkState::Missing,
                |_| false,
                no_resolve,
            ),
            Ok(()),
        );
    }

    fn targets(path: &str) -> LinkState {
        LinkState::Targets(PathBuf::from(path))
    }

    #[test]
    fn an_all_users_link_naming_this_exe_allows() {
        let this_exe = Path::new(THIS_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &targets(THIS_EXE),
                &LinkState::Missing,
                |_| true,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_all_users_link_naming_another_existing_exe_refuses() {
        let this_exe = Path::new(THIS_EXE);
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &targets(OTHER_EXE),
                &LinkState::Missing,
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::AllUsers, PathBuf::from(OTHER_EXE))),
        );
    }

    #[test]
    fn an_all_users_link_naming_a_gone_exe_allows() {
        let this_exe = Path::new(THIS_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &targets(OTHER_EXE),
                &LinkState::Missing,
                |_| false,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_unreadable_all_users_link_refuses() {
        let this_exe = Path::new(THIS_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &LinkState::Unreadable,
                &LinkState::Missing,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::AllUsers)),
        );
    }

    #[test]
    fn an_installed_copy_refused_by_an_unreadable_all_users_link_does_not_overwrite() {
        assert_eq!(
            check_existing_links(
                Path::new(INSTALLED_EXE),
                &test_roots(),
                &LinkState::Unreadable,
                &LinkState::Missing,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::AllUsers)),
        );
    }

    #[test]
    fn a_per_user_link_naming_another_existing_exe_refuses() {
        let this_exe = Path::new(THIS_EXE);
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &LinkState::Missing,
                &targets(OTHER_EXE),
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::PerUser, PathBuf::from(OTHER_EXE))),
        );
    }

    #[test]
    fn an_installed_copy_is_allowed_past_another_exes_link() {
        let installed = Path::new(r"C:\Users\Fredi\AppData\Local\Programs\Omairc\omairc.exe");
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                installed,
                &test_roots(),
                &targets(OTHER_EXE),
                &LinkState::Missing,
                |path| path == other,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_empty_target_is_unreadable() {
        assert_eq!(link_state_from_target(Some(""), identity), LinkState::Unreadable);
    }

    #[test]
    fn a_failed_target_read_is_unreadable() {
        assert_eq!(link_state_from_target(None, identity), LinkState::Unreadable);
    }

    #[test]
    fn an_unexpanded_variable_is_unreadable() {
        assert_eq!(link_state_from_target(Some(ROOT_TARGET), identity), LinkState::Unreadable);
    }

    #[test]
    fn a_target_expanding_to_another_existing_exe_refuses() {
        let state = link_state_from_target(Some(ROOT_TARGET), expand_root);
        assert_eq!(state, targets(OTHER_EXE));
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &[],
                &LinkState::Missing,
                &state,
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::PerUser, PathBuf::from(OTHER_EXE))),
        );
    }

    #[test]
    fn a_target_expanding_to_a_gone_exe_is_claimed() {
        let state = link_state_from_target(Some(ROOT_TARGET), expand_root);
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &[],
                &LinkState::Missing,
                &state,
                |_| false,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_expansion_failure_refuses() {
        let state = link_state_from_target(Some(ROOT_TARGET), |_| None);
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &[],
                &LinkState::Missing,
                &state,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::PerUser)),
        );
    }

    /// Expands the one variable these tests use, `%ROOT%`, to the other install's folder.
    fn expand_root(raw: &str) -> Option<String> {
        Some(raw.replace("%ROOT%", r"C:\Program Files\Other"))
    }

    fn identity(raw: &str) -> Option<String> {
        Some(raw.to_owned())
    }

    #[test]
    fn an_unreadable_per_user_link_refuses() {
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &[],
                &LinkState::Missing,
                &LinkState::Unreadable,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::PerUser)),
        );
    }

    #[test]
    fn an_installed_copy_refused_by_an_unreadable_link_does_not_overwrite() {
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                Path::new(INSTALLED_EXE),
                &test_roots(),
                &targets(OTHER_EXE),
                &LinkState::Unreadable,
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::PerUser)),
        );
    }

    #[test]
    fn when_both_links_refuse_the_all_users_refusal_is_reported() {
        let this_exe = Path::new(THIS_EXE);
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &LinkState::Unreadable,
                &LinkState::Unreadable,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::AllUsers)),
        );
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &targets(OTHER_EXE),
                &LinkState::Unreadable,
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::AllUsers, PathBuf::from(OTHER_EXE))),
        );
    }

    /// A scratch folder under the system temp folder, removed when the test ends. Each test makes
    /// its own, so tests stay isolated, and none touches the Start Menu, the registry, or an
    /// install folder.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("omairc-notifier-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch folder");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        /// The path of `<folder>\omairc.exe` under this folder. The file is written when
        /// `present`, so the path is a real exe or a real absence.
        fn exe(&self, folder: &str, present: bool) -> PathBuf {
            let dir = self.0.join(folder);
            let exe = dir.join("omairc.exe");
            if present {
                std::fs::create_dir_all(&dir).expect("create scratch exe folder");
                std::fs::write(&exe, b"").expect("write scratch exe");
            }
            exe
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Runs an installed copy at `exe` through the post-repair gate and the fallback's check, with
    /// `roots` as its install roots and its own all-users link stamped, so the repair ran. The gate
    /// must go on, and the check must allow the per-user link to be replaced.
    fn assert_installed_copy_replaces(exe: &Path, roots: &[PathBuf], per_user: &LinkState) {
        let from_install = is_installed_exe(exe, roots, no_resolve);
        assert!(from_install, "{} must count as installed", exe.display());
        assert!(
            !repair_ends_registration(true, from_install, exe, per_user),
            "the gate stopped for {per_user:?}"
        );
        assert_eq!(
            check_existing_links(
                exe,
                roots,
                &LinkState::Targets(exe.to_path_buf()),
                per_user,
                Path::exists,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_installed_copy_replaces_a_per_user_link_naming_a_still_existing_exe() {
        let scratch = ScratchDir::new("replace_existing");
        let installed = scratch.exe("Omairc", true);
        let roots = [scratch.path().join("Omairc")];
        let other = scratch.exe("Other", true);
        assert!(other.exists(), "the other exe must be on disk");
        assert_installed_copy_replaces(&installed, &roots, &LinkState::Targets(other));
    }

    #[test]
    fn an_installed_copy_replaces_a_per_user_link_naming_a_gone_exe() {
        let scratch = ScratchDir::new("replace_gone");
        let installed = scratch.exe("Omairc", true);
        let roots = [scratch.path().join("Omairc")];
        let gone = scratch.exe("Other", false);
        assert!(!gone.exists(), "the other exe must be absent");
        assert_installed_copy_replaces(&installed, &roots, &LinkState::Targets(gone));
    }

    #[test]
    fn an_installed_copy_whose_per_user_link_names_its_existing_exe_stops_with_no_write() {
        let scratch = ScratchDir::new("stop_named");
        let installed = scratch.exe("Omairc", true);
        let roots = [scratch.path().join("Omairc")];
        assert!(installed.exists(), "the installed exe must be on disk");
        let from_install = is_installed_exe(&installed, &roots, no_resolve);
        assert!(from_install, "the copy must count as installed");
        assert!(repair_ends_registration(
            true,
            from_install,
            &installed,
            &LinkState::Targets(installed.clone()),
        ));
    }

    #[test]
    fn a_dev_build_does_not_overwrite_a_dev_per_user_link_past_its_own_all_users_link() {
        // Its own all-users link is stamped, so the repair runs. A dev build stops there,
        // before the per-user decision.
        assert!(stops_after_repair(THIS_EXE, true, &targets(DEV_EXE)));
    }

    /// Runs the post-repair gate as `ensure_notification_registration` does, with
    /// `from_install` taken from the exe's location.
    fn stops_after_repair(exe: &str, repaired: bool, per_user: &LinkState) -> bool {
        let this_exe = Path::new(exe);
        let from_install = is_installed_exe(this_exe, &test_roots(), no_resolve);
        repair_ends_registration(repaired, from_install, this_exe, per_user)
    }

    /// The per-user link states a copy at `exe` can meet: missing, unreadable, naming `exe`
    /// itself, and naming another exe.
    fn per_user_states_for(exe: &str) -> [LinkState; 4] {
        [
            LinkState::Missing,
            LinkState::Unreadable,
            targets(exe),
            targets(OTHER_EXE),
        ]
    }

    #[test]
    fn without_a_repair_nothing_stops_for_any_per_user_link() {
        for exe in [THIS_EXE, INSTALLED_EXE] {
            for state in per_user_states_for(exe) {
                assert!(
                    !stops_after_repair(exe, false, &state),
                    "{exe} with {state:?} stopped without a repair"
                );
            }
        }
    }

    #[test]
    fn a_dev_build_stops_after_a_repair_for_any_per_user_link() {
        for state in per_user_states_for(THIS_EXE) {
            assert!(
                stops_after_repair(THIS_EXE, true, &state),
                "dev build with {state:?} went on after a repair"
            );
        }
    }

    #[test]
    fn an_installed_copy_stops_after_a_repair_when_its_per_user_link_is_missing() {
        assert!(stops_after_repair(INSTALLED_EXE, true, &LinkState::Missing));
    }

    #[test]
    fn an_installed_copy_stops_after_a_repair_when_its_per_user_link_names_this_exe() {
        assert!(stops_after_repair(INSTALLED_EXE, true, &targets(INSTALLED_EXE)));
        // The comparison is by path, so another letter case of the same exe also stops.
        assert!(stops_after_repair(
            INSTALLED_EXE,
            true,
            &targets(r"c:\program files\omairc\omairc.exe"),
        ));
    }

    #[test]
    fn an_installed_copy_stops_after_a_repair_when_its_per_user_link_is_unreadable() {
        assert!(stops_after_repair(INSTALLED_EXE, true, &LinkState::Unreadable));
    }

    #[test]
    fn found_after_repair_with_no_links_is_false_and_writes_nothing() {
        let attempts = RefCell::new(Vec::new());
        let found = found_after_repair(
            &[],
            |_| false,
            |link| {
                attempts.borrow_mut().push(link.to_path_buf());
                true
            },
        );
        assert!(!found);
        assert!(attempts.borrow().is_empty());
    }

    #[test]
    fn a_stamped_link_is_found_and_nothing_is_written() {
        let link = PathBuf::from("all-users.lnk");
        let attempts = RefCell::new(Vec::new());
        let found = found_after_repair(
            std::slice::from_ref(&link),
            |_| true,
            |path| {
                attempts.borrow_mut().push(path.to_path_buf());
                true
            },
        );
        assert!(found);
        assert!(attempts.borrow().is_empty());
    }

    #[test]
    fn a_link_found_by_name_counts_even_when_its_stamp_fails() {
        let link = PathBuf::from("all-users.lnk");
        let attempts = RefCell::new(Vec::new());
        let found = found_after_repair(
            std::slice::from_ref(&link),
            |_| false,
            |path| {
                attempts.borrow_mut().push(path.to_path_buf());
                false
            },
        );
        assert!(found, "a failed stamp must not make the link disappear");
        assert_eq!(attempts.into_inner(), vec![link]);
    }

    #[test]
    fn a_stamped_link_among_unstamped_ones_stops_the_writes() {
        let first = PathBuf::from("all-users.lnk");
        let second = PathBuf::from("per-user.lnk");
        let attempts = RefCell::new(Vec::new());
        let found = found_after_repair(
            &[first.clone(), second.clone()],
            |path| path == second.as_path(),
            |path| {
                attempts.borrow_mut().push(path.to_path_buf());
                true
            },
        );
        assert!(found);
        assert!(attempts.borrow().is_empty(), "no stamp is written when one link is stamped");
    }

    #[test]
    fn the_first_stamp_that_takes_ends_the_attempts() {
        let first = PathBuf::from("all-users.lnk");
        let second = PathBuf::from("per-user.lnk");
        let attempts = RefCell::new(Vec::new());
        let found = found_after_repair(
            &[first.clone(), second],
            |_| false,
            |path| {
                attempts.borrow_mut().push(path.to_path_buf());
                true
            },
        );
        assert!(found);
        assert_eq!(attempts.into_inner(), vec![first]);
    }

    #[test]
    fn a_failed_stamp_moves_on_to_the_next_link() {
        let first = PathBuf::from("all-users.lnk");
        let second = PathBuf::from("per-user.lnk");
        let attempts = RefCell::new(Vec::new());
        let found = found_after_repair(
            &[first.clone(), second.clone()],
            |_| false,
            |path| {
                attempts.borrow_mut().push(path.to_path_buf());
                path == second.as_path()
            },
        );
        assert!(found);
        assert_eq!(attempts.into_inner(), vec![first, second]);
    }

    #[test]
    fn an_unstamped_all_users_link_naming_this_exe_blocks_the_per_user_fallback_when_its_stamp_fails() {
        // The all-users install's link names this exe, but stamping it needs admin rights and the
        // stamp fails. The link is still found, so the installed copy stops and makes no per-user
        // link.
        let all_users = PathBuf::from(
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Omairc.lnk",
        );
        let repaired = found_after_repair(&[all_users], |_| false, |_| false);
        assert!(repaired);
        assert!(stops_after_repair(INSTALLED_EXE, repaired, &LinkState::Missing));
    }
}
