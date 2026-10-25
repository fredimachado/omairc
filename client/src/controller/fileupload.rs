//! FILEHOST endpoint and credential snapshots for the shell's upload queue.
//! Ports `tui/internal/controller/fileupload.go` without owning the HTTP work.

use crate::util::Secret;

use super::controller::Controller;

/// One upload the desktop layer may POST.
#[derive(Clone, Debug)]
pub struct UploadTarget {
    pub endpoint: String,
    pub user: String,
    pub secret: Secret,
    pub server_host: String,
    pub server_encrypted: bool,
}

impl UploadTarget {
    /// Clears the password and user name. The `Drop` impl runs it, and the upload runs it
    /// once the headers are built, so the plaintext is not held across the post.
    pub fn wipe_credentials(&mut self) {
        self.secret.wipe();
        self.user.clear();
    }
}

/// `upload` consumes its job, so every return drops the target, including a refused
/// pre-check, and that drop clears the credentials. No exit has to remember to wipe.
impl Drop for UploadTarget {
    fn drop(&mut self) {
        self.wipe_credentials();
    }
}

impl Controller {
    /// The upload URI for the selected network, or `None`. An encrypted IRC
    /// connection refuses a plain http URI. With no live session the connection
    /// is treated as encrypted, so only https is offered.
    pub fn file_host(&self) -> Option<String> {
        let network_id = self.upload_network_id()?;
        let encrypted = self
            .session(&network_id)
            .map(|session| session.tls_enabled())
            .unwrap_or(true);
        self.reducer
            .server_features(&network_id)
            .file_host(encrypted)
            .map(str::to_owned)
    }

    /// The endpoint and credentials for `network_id`, when that network advertises a host.
    pub fn upload_target(&self, network_id: &str) -> Option<UploadTarget> {
        let encrypted = self
            .session(network_id)
            .map(|session| session.tls_enabled())
            .unwrap_or(true);
        let endpoint = self
            .reducer
            .server_features(network_id)
            .file_host(encrypted)?
            .to_owned();
        let (user, secret, server_host, server_encrypted) = match self.session(network_id) {
            Some(session) => {
                let (user, secret, host, encrypted) = session.upload_credential();
                (user, secret, host, encrypted)
            }
            None => (String::new(), Secret::default(), String::new(), true),
        };
        Some(UploadTarget {
            endpoint,
            user,
            secret,
            server_host,
            server_encrypted,
        })
    }

    /// Records a fixed failure line on Status for `network_id`.
    pub fn note_file_upload_failure(&mut self, network_id: &str, text: &str) {
        if text.is_empty() || network_id.is_empty() {
            return;
        }
        self.log_outcome(network_id, text);
    }

    pub fn upload_network_id(&self) -> Option<String> {
        if self.console_open {
            let id = self.console_network.clone();
            return (!id.is_empty()).then_some(id);
        }
        self.selected()
            .map(|key| key.network_id.clone())
            .filter(|id| !id.is_empty())
    }
}
