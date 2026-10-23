//! The shell's FILEHOST upload queue: path pick, paste, drop and background POST.

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use gpui::{
    ClipboardEntry, Context, ExternalPaths, Focusable, ImageFormat, PathPromptOptions, Window,
};

use crate::controller::fileupload::UploadTarget;
use crate::desktop::fileupload::{UploadJob, UploadSource, COULD_NOT_UPLOAD};
use crate::desktop::httpfetch::UreqFetch;
use crate::model::composer::DraftKey;
use crate::model::filelink::{local_file_lines, non_regular_paste_paths, PathStat};
use crate::ui::input::Paste;

use super::Shell;

#[derive(Clone, Debug)]
pub(super) struct QueuedFile {
    pub source: UploadSource,
    pub file_name: String,
    pub content_type: String,
    pub draft_key: DraftKey,
    pub network_id: String,
}

pub(super) struct FileUploadQueue {
    pub queue: VecDeque<QueuedFile>,
    pub active: Option<QueuedFile>,
}

impl FileUploadQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            active: None,
        }
    }
}

impl Shell {
    pub(super) fn file_upload_offered(&self) -> bool {
        self.controller.file_host().is_some()
    }

    pub(super) fn prompt_for_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.file_upload_offered() || self.find.is_some() || self.connect_open {
            return;
        }
        let draft_key = self.shown.draft_key();
        let network_id = self
            .controller
            .upload_network_id()
            .unwrap_or_else(|| self.shown.conversation.network_id.clone());
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a file".into()),
        });
        cx.spawn(async move |shell, cx| {
            let paths = rx
                .await
                .ok()
                .and_then(|result| result.ok())
                .flatten();
            if let Some(paths) = paths {
                shell
                    .update(cx, |shell, cx| {
                        shell.enqueue_paths(paths, draft_key, network_id, cx);
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(super) fn handle_composer_paste(
        &mut self,
        _: &Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.connect_field_focused(window, cx) {
            return self.swallow_file_paste(cx);
        }
        if !self.composer_focused(window, cx) || !self.file_upload_offered() || self.find.is_some() {
            return false;
        }
        let item = cx.read_from_clipboard();
        if let Some(text) = item.as_ref().and_then(|item| item.text()) {
            if self.try_enqueue_pasted_text(&text, cx) {
                return true;
            }
        } else if let Some(item) = item {
            if let Some(bytes) = clipboard_png(item) {
                self.enqueue_bytes(bytes, "image.png", "image/png", cx);
                return true;
            }
        }
        false
    }

    pub(super) fn handle_external_drop(
        &mut self,
        paths: &ExternalPaths,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.file_upload_gate(window, cx) {
            return;
        }
        let draft_key = self.shown.draft_key();
        let network_id = self
            .controller
            .upload_network_id()
            .unwrap_or_else(|| self.shown.conversation.network_id.clone());
        self.enqueue_paths(paths.paths().to_vec(), draft_key, network_id, cx);
    }

    fn file_upload_gate(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.file_upload_offered()
            && !self.connect_open
            && self.find.is_none()
            && self.composer_focused(window, cx)
    }

    fn swallow_file_paste(&mut self, cx: &mut Context<Self>) -> bool {
        let item = cx.read_from_clipboard();
        let text = item.as_ref().and_then(|item| item.text());
        if let Some(text) = text {
            let stat = path_stat;
            if local_file_lines(&text, &stat).is_some()
                || non_regular_paste_paths(&text, &stat).is_some()
            {
                return true;
            }
        }
        false
    }

    fn try_enqueue_pasted_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let stat = path_stat;
        if let Some(paths) = local_file_lines(text, &stat) {
            let draft_key = self.shown.draft_key();
            let network_id = self
                .controller
                .upload_network_id()
                .unwrap_or_else(|| self.shown.conversation.network_id.clone());
            self.enqueue_paths(paths, draft_key, network_id, cx);
            return true;
        }
        if let Some(bad) = non_regular_paste_paths(text, &stat) {
            let draft_key = self.shown.draft_key();
            let network_id = self
                .controller
                .upload_network_id()
                .unwrap_or_else(|| self.shown.conversation.network_id.clone());
            self.enqueue_paths(bad, draft_key, network_id, cx);
            return true;
        }
        false
    }

    fn enqueue_paths(
        &mut self,
        paths: Vec<PathBuf>,
        draft_key: DraftKey,
        network_id: String,
        cx: &mut Context<Self>,
    ) {
        for path in paths {
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_owned());
            let content_type = mime_guess::from_path(&path)
                .first_raw()
                .unwrap_or("application/octet-stream")
                .to_owned();
            self.file_upload.queue.push_back(QueuedFile {
                source: UploadSource::Path(path),
                file_name,
                content_type,
                draft_key: draft_key.clone(),
                network_id: network_id.clone(),
            });
        }
        self.pump_file_uploads(cx);
    }

    fn enqueue_bytes(
        &mut self,
        bytes: Vec<u8>,
        file_name: &str,
        content_type: &str,
        cx: &mut Context<Self>,
    ) {
        let draft_key = self.shown.draft_key();
        let network_id = self
            .controller
            .upload_network_id()
            .unwrap_or_else(|| self.shown.conversation.network_id.clone());
        self.file_upload.queue.push_back(QueuedFile {
            source: UploadSource::Bytes(bytes),
            file_name: file_name.to_owned(),
            content_type: content_type.to_owned(),
            draft_key,
            network_id,
        });
        self.pump_file_uploads(cx);
    }

    fn pump_file_uploads(&mut self, cx: &mut Context<Self>) {
        if self.file_upload.active.is_some() {
            return;
        }
        while let Some(entry) = self.file_upload.queue.pop_front() {
            let Some(target) = self.controller.upload_target(&entry.network_id) else {
                self.controller
                    .note_file_upload_failure(&entry.network_id, COULD_NOT_UPLOAD);
                continue;
            };
            self.start_upload_job(entry, target, cx);
            break;
        }
        cx.notify();
    }

    fn start_upload_job(
        &mut self,
        entry: QueuedFile,
        target: UploadTarget,
        cx: &mut Context<Self>,
    ) {
        self.file_upload.active = Some(entry.clone());
        let job = UploadJob {
            source: entry.source.clone(),
            file_name: entry.file_name.clone(),
            content_type: entry.content_type.clone(),
            target,
        };
        cx.spawn(async move |shell, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { crate::desktop::fileupload::upload(&UreqFetch, job) })
                .await;
            shell
                .update(cx, |shell, cx| {
                    shell.finish_upload(entry, outcome, cx);
                })
                .ok();
        })
        .detach();
    }

    fn finish_upload(
        &mut self,
        entry: QueuedFile,
        outcome: Result<String, &'static str>,
        cx: &mut Context<Self>,
    ) {
        self.file_upload.active = None;
        match outcome {
            Ok(url) => self.insert_uploaded_link(&entry, &url, cx),
            Err(message) => self
                .controller
                .note_file_upload_failure(&entry.network_id, message),
        }
        self.pump_file_uploads(cx);
    }

    fn insert_uploaded_link(&mut self, entry: &QueuedFile, url: &str, cx: &mut Context<Self>) {
        let current = self.shown.draft_key();
        let find_active = self.find.is_some();
        let (text, cursor) = self
            .composer
            .read_with(cx, |input, _| (input.text().to_string(), input.cursor()));
        if let Some(edit) = self.composer_state.append_file_link(
            &current,
            &entry.draft_key,
            find_active,
            &text,
            cursor,
            url,
        ) {
            let draft = edit.text.clone();
            self.composer.update(cx, |input, cx| {
                input.set_text_with_cursor(edit.text, edit.cursor);
                cx.notify();
            });
            self.composer_state.stash(&draft);
        }
        cx.notify();
    }

    fn composer_focused(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.composer.read(cx).focus_handle(cx).is_focused(window)
    }

    fn connect_field_focused(&self, window: &Window, cx: &Context<Self>) -> bool {
        if !self.connect_open {
            return false;
        }
        self.connect_ui
            .fields()
            .iter()
            .any(|(_, input)| input.read(cx).focus_handle(cx).is_focused(window))
    }
}

fn path_stat(path: &std::path::Path) -> Option<PathStat> {
    fs::metadata(path).ok().map(|info| PathStat {
        regular: info.is_file(),
    })
}

fn clipboard_png(item: gpui::ClipboardItem) -> Option<Vec<u8>> {
    for entry in item.entries() {
        if let ClipboardEntry::Image(image) = entry {
            if image.format == ImageFormat::Png {
                return Some(image.bytes.clone());
            }
            if let Ok(decoded) = image::load_from_memory(&image.bytes) {
                let mut png = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut png);
                if decoded
                    .write_to(&mut cursor, image::ImageFormat::Png)
                    .is_ok()
                {
                    return Some(png);
                }
            }
        }
    }
    None
}
