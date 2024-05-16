use std::collections::HashMap;
use std::num::NonZeroUsize;

use itertools::Itertools;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::{Array, Context, KeyValue, StringValue, Value};
use sqlx::types::time::{OffsetDateTime, UtcOffset};

use crate::pacs_file::PacsFileRegistrationRequest;

/// A client which writes to The _ChRIS_ backend's PostgreSQL database.
pub(crate) struct CubePostgresClient {
    /// PostgreSQL database client
    pool: sqlx::PgPool,
    /// The pacsfiles_pacs table, which maps string PACS names to integer IDs
    pacs: HashMap<String, u32>,
    /// Timezone for the "creation_date" field.
    tz: Option<UtcOffset>,
}

/// Error registering PACS files to the database.
#[derive(thiserror::Error, Debug)]
pub enum PacsFileDatabaseError {
    #[error("Wrong number of rows were affected. Tried to register {count} files, however {rows_affected} rows affected.")]
    WrongNumberOfAffectedRows {
        /// Number of files which need to be registered
        count: NonZeroUsize,
        /// Number of rows affected by execution of SQL INSERT statement
        rows_affected: u64,
    },
    #[error(transparent)]
    SqlxError(#[from] sqlx::Error)
}

impl CubePostgresClient {
    /// Constructor
    pub fn new(pool: sqlx::PgPool, tz: Option<UtcOffset>) -> Self {
        Self {
            pool,
            pacs: Default::default(),
            tz,
        }
    }

    /// Register DICOM file metadata to CUBE's database. Any files which already exist
    /// in the database will not be registered again.
    ///
    /// The SQL transaction will be committed if-*and-only-if* the INSERT is successful
    /// and the number of rows affected is expected.
    pub async fn register(
        &mut self,
        files: &[PacsFileRegistrationRequest],
    ) -> Result<(), PacsFileDatabaseError> {
        let mut transaction = self.pool.begin().await?;
        let unregistered_files =
            warn_and_remove_already_registered(&mut transaction, files).await?;
        if let Some((count, rows_affected)) = insert_into_pacsfile(&mut transaction, unregistered_files, self.get_now()).await? {
            if count.get() as u64 == rows_affected {
                transaction.commit().await.map_err(PacsFileDatabaseError::from)
            } else {
                Err(PacsFileDatabaseError::WrongNumberOfAffectedRows { count, rows_affected })
            }
        } else {
            Ok(())
        }
    }

    /// Get the current time in the local timezone.
    fn get_now(&self) -> OffsetDateTime {
        let now = OffsetDateTime::now_utc();
        if let Some(tz) = self.tz {
            now.checked_to_offset(tz).unwrap_or(now)
        } else {
            now
        }
    }
}

const PACSFILE_INSERT_STATEMENT: &str = r#"INSERT INTO pacsfiles_pacsfile (
creation_date, fname, "PatientID", "PatientName", "StudyInstanceUID", "StudyDescription",
"SeriesInstanceUID", "SeriesDescription", pacs_id, "PatientAge", "PatientBirthDate",
"PatientSex", "Modality", "ProtocolName", "StudyDate", "AccessionNumber") VALUES"#;
const PACSFILE_INSERT_TUPLE_LENGTH: NonZeroUsize = match NonZeroUsize::new(16) {
    // https://ao.owo.si/questions/66838439/how-can-i-safely-initialise-a-constant-of-type-nonzerou8
    Some(n) => n,
    None => [][0],
};

/// Execute the SQL `INSERT INTO pacsfiles_pacsfile ...` command, which registers files to CUBE's
/// database.
///
/// Does nothing if `files` is empty.
///
/// Returns the number of files, and the number of rows affected. Pro-tip: if these two values
/// are not equal, something is seriously wrong.
async fn insert_into_pacsfile<'a>(
    transaction: &mut sqlx::Transaction<'a, sqlx::Postgres>,
    files: Vec<&'a PacsFileRegistrationRequest>,
    creation_date: OffsetDateTime,
) -> Result<Option<(NonZeroUsize, u64)>, sqlx::Error> {
    if let Some(count) = NonZeroUsize::new(files.len()) {
        let statement = prepared_statement_for(
            PACSFILE_INSERT_STATEMENT,
            PACSFILE_INSERT_TUPLE_LENGTH,
            count,
        );
        let query = files.iter().fold(
            sqlx::query(&statement),
            |query, file| {
                // N.B. order must exactly match PACSFILE_INSERT_STATEMENT
                query
                    .bind(creation_date)
                    .bind(&file.path)
                    .bind(&file.PatientID)
                    .bind(file.PatientName.as_ref())
                    .bind(&file.StudyInstanceUID)
                    .bind(file.StudyDescription.as_ref())
                    .bind(&file.SeriesInstanceUID)
                    .bind(file.SeriesDescription.as_ref())
                    .bind(file.pacs_name.as_str())
                    .bind(file.PatientName.as_ref())
                    .bind(file.PatientBirthDate.as_ref())
                    .bind(file.PatientSex.as_ref())
                    .bind(file.Modality.as_ref())
                    .bind(file.ProtocolName.as_ref())
                    .bind(&file.StudyDate)
                    .bind(file.AccessionNumber.as_ref())
            }
        );
        let result = query.execute(&mut **transaction).await?;
        Ok(Some((count, result.rows_affected())))
    } else {
        Ok(None)
    }
}

/// Outcome of attempting to register files to the database.
///
/// Pro-tip: if `new_files_count != rows_affected`, something is seriously broken!
pub(crate) struct InsertionOutcome {
    /// Number of new files registered to the database
    count: NonZeroUsize,
    /// Number of database rows affected
    rows_affected: u64,
}

/// Query the database to check whether any of the files are already registered.
/// If so, show a warning about it, and exclude that file from the return value.
async fn warn_and_remove_already_registered<'a>(
    transaction: &mut sqlx::Transaction<'a, sqlx::Postgres>,
    files: &'a [PacsFileRegistrationRequest],
) -> Result<Vec<&'a PacsFileRegistrationRequest>, sqlx::Error> {
    let currently_registered = query_for_existing(transaction, files).await?;
    let already_registered_paths: Vec<_> = files
        .iter()
        .filter(|file| currently_registered.contains(&file.path))
        .map(|file| file.path.as_str())
        .collect();
    let unregistered_files = files
        .iter()
        .filter(|file| !already_registered_paths.contains(&file.path.as_str()))
        .collect();
    report_already_registered_files_via_opentelemetry(&already_registered_paths).await;
    Ok(unregistered_files)
}

/// If given a non-empty array of paths, report it to OpenTelemetry as a string array.
async fn report_already_registered_files_via_opentelemetry(already_registered_files: &[&str]) {
    if already_registered_files.is_empty() {
        return;
    }
    let string_values: Vec<_> = already_registered_files
        .iter()
        .map(|s| s.to_string())
        .map(StringValue::from)
        .collect();
    let value = Value::Array(Array::String(string_values));
    let context = Context::current();
    context
        .span()
        .set_attribute(KeyValue::new("already_registered_paths", value))
}

/// Create a SQL prepared statement with `len` parameters.
fn prepared_statement_for(statement: &str, tuple_len: NonZeroUsize, len: NonZeroUsize) -> String {
    let tuples = (0..len.get())
        .map(|n| {
            let start = n * tuple_len.get() + 1;
            let end = start + tuple_len.get();
            let placeholders = (start..end).map(|i| format!("${i}")).join(",");
            format!("({placeholders})")
        })
        .join(" ");
    format!("{statement} {tuples}")
}

/// Query the database for fnames which may already exist.
async fn query_for_existing(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    files: &[PacsFileRegistrationRequest],
) -> Result<Vec<String>, sqlx::Error> {
    if let Some(n_files) = NonZeroUsize::new(files.len()) {
        let statement = prepared_statement_for(
            "SELECT fname FROM pacsfile_pacsfile WHERE fname IN",
            n_files,
            n_files,
        );
        let query = files.iter().fold(sqlx::query_scalar(&statement), |q, f| {
            q.bind::<&str>(f.path.as_str())
        });
        query.fetch_all(&mut **transaction).await
    } else {
        Ok(Vec::with_capacity(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statement_for1() {
        let expected1 = format!(
            "{} ({})",
            PACSFILE_INSERT_STATEMENT,
            (1..=16).map(|i| format!("${i}")).join(",")
        );
        let actual1 = prepared_statement_for(
            PACSFILE_INSERT_STATEMENT,
            PACSFILE_INSERT_TUPLE_LENGTH,
            NonZeroUsize::new(1).unwrap(),
        );
        assert_eq!(actual1, expected1);
    }

    #[test]
    fn test_statement_for3() {
        let expected1 = format!(
            "{} ({}) ({}) ({})",
            PACSFILE_INSERT_STATEMENT,
            (1..=16).map(|i| format!("${i}")).join(","),
            (17..=32).map(|i| format!("${i}")).join(","),
            (33..=48).map(|i| format!("${i}")).join(","),
        );
        let actual1 = prepared_statement_for(
            PACSFILE_INSERT_STATEMENT,
            PACSFILE_INSERT_TUPLE_LENGTH,
            NonZeroUsize::new(3).unwrap(),
        );
        assert_eq!(actual1, expected1);
    }
}
