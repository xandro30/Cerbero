//! The enrollment throttle (FR-034, research R13) in its own table, `enroll_attempts`, apart from
//! the shared counter of unsealing; it survives restarts. The streams of the identities and their
//! certificates are served by [`super::ca::SqliteIdentityStore`], whose one transaction covers
//! every stream an enrollment touches.

use std::path::Path;
use std::sync::Mutex;

use hecate_application::errors::ServiceError;
use hecate_application::ports::{EnrollAttempts, EnrollAttemptsRepository};
use hecate_domain::instant::{isoformat, parse_iso};
use rusqlite::{Connection, OptionalExtension, params};

use super::secrets::{corrupt, integer, lock, natural, store_error};
use super::{OpenError, open_database};

pub struct SqliteEnrollAttempts {
    connection: Mutex<Connection>,
}

impl SqliteEnrollAttempts {
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        Ok(Self {
            connection: Mutex::new(open_database(path)?),
        })
    }
}

impl EnrollAttemptsRepository for SqliteEnrollAttempts {
    fn load(&self) -> Result<EnrollAttempts, ServiceError> {
        let row = lock(&self.connection)
            .query_row(
                "SELECT consecutive_failures, wait_until, rejected_during_wait
                 FROM enroll_attempts WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(store_error)?;
        let Some((failures, wait_until, rejected)) = row else {
            return Ok(EnrollAttempts::default());
        };
        Ok(EnrollAttempts {
            consecutive_failures: natural(failures)?,
            wait_until: wait_until
                .as_deref()
                .map(|text| parse_iso(text).map_err(|_| corrupt()))
                .transpose()?,
            rejected_during_wait: natural(rejected)?,
        })
    }

    fn save(&self, attempts: &EnrollAttempts) -> Result<(), ServiceError> {
        lock(&self.connection)
            .execute(
                "INSERT INTO enroll_attempts (id, consecutive_failures, wait_until, rejected_during_wait)
                 VALUES (1, ?1, ?2, ?3)
                 ON CONFLICT (id) DO UPDATE SET consecutive_failures = excluded.consecutive_failures,
                 wait_until = excluded.wait_until,
                 rejected_during_wait = excluded.rejected_during_wait",
                params![
                    integer(attempts.consecutive_failures)?,
                    attempts.wait_until.map(isoformat),
                    integer(attempts.rejected_during_wait)?,
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }
}
