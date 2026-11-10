//! The task of every minute (research R3, R8, R13): it forgets a ceremony that expired or belongs
//! to an unseal that is over, drops the short call tokens that can no longer be accepted, closes
//! the batch of the enrollment throttle whose wait is over, destroys the key of an intermediate
//! whose last certificate expired (FR-005), renews Hécate's certificate when it is due and keeps
//! the API listening only with a valid certificate. When the last certificate of a previous
//! intermediate expires before the next run, its key goes at that instant.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hecate_application::ca::service::CaService;
use hecate_application::identity::service::IdentityService;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, Sleep};

use crate::api::supervisor::ApiSupervisor;

pub const PERIOD: Duration = Duration::from_secs(60);

/// The running task, until [`Periodic::stop`].
pub struct Periodic {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl Periodic {
    pub async fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.task.await;
    }
}

/// Destroys the keys of the previous intermediates whose last certificate expired; returns
/// whether it could.
async fn retire(ca: &Arc<CaService>) -> bool {
    let ca = ca.clone();
    match tokio::task::spawn_blocking(move || ca.retire_intermediates()).await {
        Ok(Ok(retired)) => {
            for (generation, version) in retired {
                tracing::info!(target: "hecate.ca", generation, version, "the key of a previous intermediate was destroyed");
            }
            true
        }
        Ok(Err(error)) => {
            tracing::error!(target: "hecate.ca", reason = %error, "cannot retire the previous intermediates");
            false
        }
        Err(_) => false,
    }
}

/// Sets `retirement` to the instant a previous key goes, when it comes before the next run;
/// returns whether it is set.
async fn arm(retirement: Pin<&mut Sleep>, ca: &Arc<CaService>, period: Duration) -> bool {
    let planner = ca.clone();
    let left = match tokio::task::spawn_blocking(move || planner.next_retirement()).await {
        Ok(Ok(left)) => left,
        Ok(Err(error)) => {
            tracing::error!(target: "hecate.ca", reason = %error, "cannot plan the retirement of the previous intermediates");
            None
        }
        Err(_) => None,
    };
    let Some(left) = left.map(|left| Duration::try_from(left).unwrap_or(Duration::ZERO)) else {
        return false;
    };
    if left >= period {
        return false;
    }
    retirement.reset(Instant::now() + left);
    true
}

pub fn spawn_periodic(
    period: Duration,
    ca: Arc<CaService>,
    identities: Arc<IdentityService>,
    supervisor: Arc<ApiSupervisor>,
) -> Periodic {
    let (stop, mut stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        interval.tick().await;
        let retirement = tokio::time::sleep(Duration::ZERO);
        tokio::pin!(retirement);
        let mut armed = arm(retirement.as_mut(), &ca, period).await;
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                () = &mut retirement, if armed => {
                    // After a failure, the next run tries again: the timer is not set at once.
                    armed = retire(&ca).await && arm(retirement.as_mut(), &ca, period).await;
                }
                _ = interval.tick() => {
                    let (blocking_ca, blocking_identities) = (ca.clone(), identities.clone());
                    let closed = tokio::task::spawn_blocking(move || {
                        blocking_ca.forget_lost_ceremony();
                        blocking_identities.purge_seen_tokens();
                        blocking_identities.close_throttle_batch()
                    })
                    .await;
                    if let Ok(Err(error)) = closed {
                        tracing::error!(target: "hecate.api", reason = %error, "cannot close the batch of the throttle");
                    }
                    let retired = retire(&ca).await;
                    if let Err(error) = supervisor.tick().await {
                        tracing::error!(target: "hecate.api", reason = %error, "the task of every minute failed");
                    }
                    armed = retired && arm(retirement.as_mut(), &ca, period).await;
                }
            }
        }
    });
    Periodic { stop, task }
}
