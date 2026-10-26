//! FILEHOST controller reads: the offer gate and failure routing.

use crate::controller::tests::fixture::{status_text, Fixture, OMARCHY};
use crate::desktop::fileupload::COULD_NOT_UPLOAD;
use crate::irc::serverfeatures::IrcServerFeatures;

#[test]
fn file_host_hidden_without_an_offer() {
    let fixture = Fixture::seeded();
    assert!(fixture.controller.file_host().is_none());
}

#[test]
fn file_upload_failure_lands_on_the_queuing_network() {
    let mut fixture = Fixture::seeded();
    let mut features = IrcServerFeatures::new();
    features.apply_token("soju.im/FILEHOST=https://uploads.example/upload");
    fixture.controller.reducer.set_server_features(OMARCHY, features);
    fixture
        .controller
        .note_file_upload_failure(OMARCHY, COULD_NOT_UPLOAD);
    let lines = status_text(&fixture.controller, OMARCHY);
    assert!(lines.iter().any(|line| line.contains(COULD_NOT_UPLOAD)));
}
