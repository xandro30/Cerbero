//! Integration helpers for the tests (feature `test-support`), as in `fakes.py`.

use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::oneshot;

use crate::control::server::{ControlServer, LineHandler};

/// `ControlServer::bind` narrows the process umask while it binds, as the Python version does.
/// A directory that another test thread creates meanwhile would lose its search bit, so the
/// binding in `ServerThread` and the directories of `short_dir` take turns.
static UMASK_TURN: Mutex<()> = Mutex::new(());

fn umask_turn() -> MutexGuard<'static, ()> {
    UMASK_TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A short temporary directory under /tmp: socket paths have a small length limit.
pub fn short_dir() -> tempfile::TempDir {
    let _turn = umask_turn();
    tempfile::Builder::new()
        .prefix("hc")
        .tempdir_in("/tmp")
        .expect("cannot create a temporary directory")
}

/// The control server running in a background thread until dropped.
pub struct ServerThread {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl ServerThread {
    pub fn start(handler: Arc<dyn LineHandler>, socket_path: &Path) -> Self {
        let turn = umask_turn();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let path = socket_path.to_path_buf();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cannot build the server runtime");
            runtime.block_on(async move {
                let server = ControlServer::bind(handler, &path).expect("cannot bind the socket");
                let _ = ready_tx.send(());
                server
                    .serve_until(async {
                        let _ = stop_rx.await;
                    })
                    .await;
            });
        });
        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("server did not start");
        drop(turn);
        Self {
            stop: Some(stop_tx),
            thread: Some(thread),
        }
    }
}

impl Drop for ServerThread {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

use hecate_application::ports::{Clock, SecretStore};
use hecate_application::sealing::service::SealingService;
use hecate_application::sealing::settings::SealingSettings;
use hecate_application::secrets::service::SecretsService;
use hecate_application::secrets::settings::SecretsSettings;
use hecate_application::testing::MINIMAL_KDF;

use crate::crypto::SoftwareCryptoProvider;
use crate::store::sealing::{SqliteAccessLog, SqliteSealRepository};
use crate::store::secrets::SqliteSecretStore;

/// The real adapters over one database file, as `hecate serve` builds them, with the cheapest
/// key derivation.
pub struct SqliteStack {
    pub clock: Arc<dyn Clock>,
    pub repo: Arc<SqliteSealRepository>,
    pub access_log: Arc<SqliteAccessLog>,
    pub store: Arc<SqliteSecretStore>,
    pub sealing: Arc<SealingService>,
    pub service: Arc<SecretsService>,
}

impl SqliteStack {
    pub fn open(db_path: &Path, clock: Arc<dyn Clock>) -> Self {
        Self::open_with(db_path, clock, |store| store)
    }

    /// The same, with the secrets service on a wrapper of the store (to make a write fail).
    pub fn open_with(
        db_path: &Path,
        clock: Arc<dyn Clock>,
        wrap: impl FnOnce(Arc<SqliteSecretStore>) -> Arc<dyn SecretStore>,
    ) -> Self {
        let open = "cannot open the database";
        let repo = Arc::new(SqliteSealRepository::open(db_path).expect(open));
        let access_log = Arc::new(SqliteAccessLog::open(db_path).expect(open));
        let store = Arc::new(SqliteSecretStore::open(db_path).expect(open));
        let crypto = Arc::new(SoftwareCryptoProvider::new());
        let sealing = Arc::new(SealingService::new(
            crypto.clone(),
            repo.clone(),
            access_log.clone(),
            clock.clone(),
            SealingSettings::new(MINIMAL_KDF),
        ));
        let service = Arc::new(SecretsService::new(
            sealing.clone(),
            wrap(store.clone()),
            crypto,
            access_log.clone(),
            clock.clone(),
            SecretsSettings::default(),
        ));
        Self {
            clock,
            repo,
            access_log,
            store,
            sealing,
            service,
        }
    }

    /// Stops as the process does: the root is released and the connections are closed.
    pub fn close(self) {
        self.sealing.shutdown();
    }
}
