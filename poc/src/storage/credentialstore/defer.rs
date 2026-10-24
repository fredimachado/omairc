//! Non-blocking reads for durable keychains. [`CredentialStore::read_deferred`] returns
//! [`ReadSlot::Loading`] while a worker runs the blocking backend read.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{mpsc, Arc};
use std::thread;

use super::{CredentialKey, CredentialState, CredentialStore, PendingRemovals, ReadSlot};

type WorkerRead = Arc<dyn Fn(&CredentialKey) -> Result<Option<String>, String> + Send + Sync>;

/// Wraps a durable store so the UI thread never waits on the keyring worker.
pub struct DeferringCredentialStore {
    inner: Box<dyn CredentialStore>,
    worker_read: WorkerRead,
    cache: RefCell<BTreeMap<CredentialKey, Result<Option<String>, String>>>,
    inflight: RefCell<BTreeMap<CredentialKey, mpsc::Receiver<Result<Option<String>, String>>>>,
}

impl DeferringCredentialStore {
    pub fn new(
        inner: Box<dyn CredentialStore>,
        worker_read: impl Fn(&CredentialKey) -> Result<Option<String>, String> + Send + Sync + 'static,
    ) -> Box<dyn CredentialStore> {
        Box::new(Self {
            inner,
            worker_read: Arc::new(worker_read),
            cache: RefCell::new(BTreeMap::new()),
            inflight: RefCell::new(BTreeMap::new()),
        })
    }

    fn drop_cached(&self, key: &CredentialKey) {
        self.cache.borrow_mut().remove(key);
        self.inflight.borrow_mut().remove(key);
    }

    fn settle_inflight(&self, key: &CredentialKey) -> Option<Result<Option<String>, String>> {
        let mut inflight = self.inflight.borrow_mut();
        let Some(receiver) = inflight.get(key) else {
            return None;
        };
        match receiver.try_recv() {
            Ok(answer) => {
                inflight.remove(key);
                self.cache.borrow_mut().insert(key.clone(), answer.clone());
                Some(answer)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                inflight.remove(key);
                Some(Err("the secret service read ended unexpectedly".to_owned()))
            }
        }
    }

    fn start_read(&self, key: &CredentialKey) {
        let (sender, receiver) = mpsc::channel();
        let worker_read = Arc::clone(&self.worker_read);
        let worker_key = key.clone();
        thread::spawn(move || {
            let _ = sender.send(worker_read(&worker_key));
        });
        self.inflight.borrow_mut().insert(key.clone(), receiver);
    }
}

impl CredentialStore for DeferringCredentialStore {
    fn durable(&self) -> bool {
        self.inner.durable()
    }

    fn read(&self, key: &CredentialKey) -> Result<Option<String>, String> {
        match self.read_deferred(key) {
            ReadSlot::Ready(answer) => answer,
            ReadSlot::Loading => Err("checking secure storage".to_owned()),
        }
    }

    fn read_deferred(&self, key: &CredentialKey) -> ReadSlot {
        if let Some(cached) = self.cache.borrow().get(key) {
            return ReadSlot::Ready(cached.clone());
        }
        if let Some(answer) = self.settle_inflight(key) {
            return ReadSlot::Ready(answer);
        }
        if !self.inflight.borrow().contains_key(key) {
            self.start_read(key);
        }
        ReadSlot::Loading
    }

    fn poll_reads(&self) {
        let keys: Vec<CredentialKey> = self.inflight.borrow().keys().cloned().collect();
        for key in keys {
            let _ = self.settle_inflight(&key);
        }
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        self.drop_cached(key);
        self.inner.write(key, password)
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        self.drop_cached(key);
        self.inner.remove(key)
    }

    fn pending(&self) -> &PendingRemovals {
        self.inner.pending()
    }

    fn pending_mut(&mut self) -> &mut PendingRemovals {
        self.inner.pending_mut()
    }
}
