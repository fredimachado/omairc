//! The shell's avatar photos. Each frame it works out which avatars are drawn, starts
//! the fetch of any https avatar that has none, and gives the views the decoded image of
//! each one that is ready. The rules (what is fetched, when, and how long a failure
//! waits) are in `controller::avatars`. The network and the decode run on the
//! background executor, in `desktop::avatarhttp`. Qt equivalents: `NickGlyph` for
//! the photo, and `IrcAvatarStore` for the fetch.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use gpui::{Context, ImageSource};

use crate::desktop::avatarhttp::AvatarJob;
use crate::model::presence::{bundled_path, photo_for};
use crate::controller::snapshots::ConversationRow;
use crate::irc::reducer::MemberView;
use crate::model::sidebar::NetworkHeader;
use crate::ui::widgets::avatar::{embedded_photo, fetched_photo, AvatarPhotos};

use super::modal::ActiveModal;
use super::Shell;

/// The decoded image of each fetched avatar, kept with the PNG it was decoded from. A
/// frame reuses the image while the controller still holds that same PNG.
pub type AvatarMemo = HashMap<String, (Arc<[u8]>, ImageSource)>;

fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

/// Adds the avatar a nick draws to the frame's set, when the preference wants a photo
/// for it. Pref off adds nothing, so nothing is fetched or drawn.
fn want_photo(wanted: &mut BTreeSet<String>, reference: Option<&str>, avatars_on: bool) {
    if let Some(reference) = photo_for(reference, avatars_on) {
        wanted.insert(reference.to_owned());
    }
}

impl Shell {
    /// The photos this frame's views draw. It collects the avatars on screen: the network
    /// heads and the footer, the direct rows, the member list (empty when its panel is
    /// shut), the transcript's authors, and an open Nick sheet's members. Then it starts
    /// any missing fetch and gives the views what is ready.
    pub(super) fn avatar_photos_for_frame(
        &mut self,
        networks: &[NetworkHeader],
        conversations: &[ConversationRow],
        members: &[MemberView],
        cx: &mut Context<Self>,
    ) -> AvatarPhotos {
        let avatars_on = self.controller.preferences().avatars;
        let mut wanted = BTreeSet::new();
        for network in networks {
            want_photo(&mut wanted, network.avatar.as_deref(), avatars_on);
        }
        for row in conversations.iter().filter(|row| row.direct) {
            want_photo(&mut wanted, non_empty(&row.avatar), avatars_on);
        }
        for member in members {
            want_photo(&mut wanted, non_empty(&member.avatar), avatars_on);
        }
        if !self.status_showing() {
            let facts = self.controller.nick_facts(&self.shown.conversation.network_id);
            let mut nicks: BTreeSet<&str> = self
                .controller
                .messages()
                .iter()
                .map(|message| message.author.as_str())
                .filter(|author| !author.is_empty())
                .collect();
            nicks.insert(self.controller.selected_target());
            for nick in nicks {
                let avatar = facts.get(nick).and_then(|facts| facts.avatar.as_deref());
                want_photo(&mut wanted, avatar, avatars_on);
            }
        }
        if let Some(ActiveModal::Nick { sheet, .. }) = &self.modal {
            for member in &sheet.read(cx).source.members {
                want_photo(&mut wanted, non_empty(&member.avatar), avatars_on);
            }
        }
        self.frame_photos(&wanted, cx)
    }

    /// Gives the photos to the views that keep their own copy: the transcript, and an
    /// open Nick sheet. Each repaints only when the references it draws have changed.
    pub(super) fn push_photos(&mut self, photos: &AvatarPhotos, cx: &mut Context<Self>) {
        self.transcript.update(cx, |transcript, cx| {
            if transcript.set_photos(photos.clone()) {
                cx.notify();
            }
        });
        if let Some(ActiveModal::Nick { sheet, .. }) = &self.modal {
            sheet.update(cx, |sheet, cx| {
                if !sheet.source.photos.same_references(photos) {
                    sheet.source.photos = photos.clone();
                    cx.notify();
                }
            });
        }
    }

    /// The photos for every reference in `wanted`. A bundled reference is embedded. An
    /// https reference shows its decoded image once the controller has kept it. Until
    /// then, and after a failed fetch, the nick keeps its initials. A reference with
    /// no photo and no fetch in flight starts one here, on the background executor.
    fn frame_photos(
        &mut self,
        wanted: &BTreeSet<String>,
        cx: &mut Context<Self>,
    ) -> AvatarPhotos {
        let mut sources = HashMap::new();
        for reference in wanted {
            if let Some(path) = bundled_path(reference) {
                sources.insert(reference.clone(), embedded_photo(path));
            } else if let Some(png) = self.controller.avatar_bytes(reference) {
                let source = match self.avatar_memo.get(reference) {
                    Some((kept, source)) if Arc::ptr_eq(kept, &png) => source.clone(),
                    _ => {
                        let source = fetched_photo(&png);
                        self.avatar_memo
                            .insert(reference.clone(), (png, source.clone()));
                        source
                    }
                };
                sources.insert(reference.clone(), source);
            } else if let Some(job) = self.controller.request_avatar(reference) {
                self.start_avatar_job(job, cx);
            }
        }
        // The memo keeps only the references this frame draws, so it stays small.
        self.avatar_memo
            .retain(|reference, _| wanted.contains(reference));
        AvatarPhotos::new(sources)
    }

    /// Runs one fetch on the background executor. When it finishes, the shell hands the
    /// outcome to the controller and repaints, so the photo replaces the initials, or
    /// the initials stay after a failure.
    fn start_avatar_job(&mut self, job: AvatarJob, cx: &mut Context<Self>) {
        cx.spawn(async move |shell, cx| {
            let (key, outcome) = cx
                .background_executor()
                .spawn(async move { job.run() })
                .await;
            shell
                .update(cx, |shell, cx| {
                    shell.controller.finish_avatar(&key, outcome);
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }
}
