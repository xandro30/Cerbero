//! The supervisor of the API (research R1, R8): it reads the CA and Hécate's certificate and
//! starts the listener, replaces its TLS configuration or closes it. `serve` calls it when it
//! starts, the channel after `ca.create.confirm` and after an unseal, and the task of every minute.
//! Without a valid certificate of its own the API does not listen, also while sealed.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use hecate_application::ca::service::{CaService, TlsMaterial};
use hecate_application::errors::ServiceError;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::sync::Mutex;

use crate::api::server::{ApiServer, RefusalRecorder, SealedCheck, ServerTls, app, server_config};

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("{0}")]
    Service(#[from] ServiceError),
    #[error("{0}")]
    Tls(#[from] rustls::Error),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("the work of the supervisor stopped")]
    Stopped,
}

/// The listener and the certificate that it presents.
struct Running {
    server: ApiServer,
    certificate: Vec<u8>,
}

pub struct ApiSupervisor {
    addr: SocketAddr,
    ca: Arc<CaService>,
    app: Router,
    running: Mutex<Option<Running>>,
}

fn server_tls(mut material: TlsMaterial) -> ServerTls {
    ServerTls {
        chain: std::mem::take(&mut material.chain)
            .into_iter()
            .map(CertificateDer::from)
            .collect(),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(material.key.expose().to_vec())),
        client_roots: vec![CertificateDer::from(std::mem::take(&mut material.root))],
    }
}

impl ApiSupervisor {
    /// The API on `addr` with `routes`; what it refuses before an operation runs goes to
    /// `refused`.
    pub fn new(
        addr: SocketAddr,
        ca: Arc<CaService>,
        routes: Router,
        sealed: SealedCheck,
        refused: RefusalRecorder,
    ) -> Self {
        Self {
            addr,
            ca,
            app: app(routes, sealed, refused),
            running: Mutex::new(None),
        }
    }

    /// Starts, replaces or stops the listener after what the CA says now.
    pub async fn refresh(&self) -> Result<(), SupervisorError> {
        let ca = self.ca.clone();
        let material = tokio::task::spawn_blocking(move || ca.tls_material())
            .await
            .map_err(|_| SupervisorError::Stopped)??;
        let mut running = self.running.lock().await;
        let Some(material) = material else {
            if let Some(current) = running.take() {
                current.server.shutdown().await;
                tracing::warn!(target: "hecate.api", "the API stops listening: Hécate has no valid certificate");
            }
            return Ok(());
        };
        let certificate = material.chain[0].clone();
        if running
            .as_ref()
            .is_some_and(|current| current.certificate == certificate)
        {
            return Ok(());
        }
        let config = Arc::new(server_config(server_tls(material))?);
        match running.as_mut() {
            Some(current) => {
                current.server.replace_config(config).await;
                current.certificate = certificate;
            }
            None => {
                let server = ApiServer::bind(self.addr, config, self.app.clone()).await?;
                tracing::info!(target: "hecate.api", addr = %server.local_addr(), "the API listens");
                *running = Some(Running {
                    server,
                    certificate,
                });
            }
        }
        Ok(())
    }

    /// Renews Hécate's certificate if it is due and refreshes the listener.
    pub async fn tick(&self) -> Result<(), SupervisorError> {
        let ca = self.ca.clone();
        tokio::task::spawn_blocking(move || ca.renew_own_certificate_if_due())
            .await
            .map_err(|_| SupervisorError::Stopped)??;
        self.refresh().await
    }

    /// Where the API listens now, if it does.
    pub async fn local_addr(&self) -> Option<SocketAddr> {
        self.running
            .lock()
            .await
            .as_ref()
            .map(|current| current.server.local_addr())
    }

    pub async fn shutdown(&self) {
        if let Some(current) = self.running.lock().await.take() {
            current.server.shutdown().await;
        }
    }
}
