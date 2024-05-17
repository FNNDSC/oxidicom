use std::collections::HashMap;
use std::num::NonZeroUsize;

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
    SqlxError(#[from] sqlx::Error),
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
        if let Some((count, rows_affected)) =
            insert_into_pacsfile(&mut transaction, unregistered_files, self.get_now()).await?
        {
            if count.get() as u64 == rows_affected {
                transaction
                    .commit()
                    .await
                    .map_err(PacsFileDatabaseError::from)
            } else {
                Err(PacsFileDatabaseError::WrongNumberOfAffectedRows {
                    count,
                    rows_affected,
                })
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
        todo!()
        // let statement = prepared_statement_for(
        //     PACSFILE_INSERT_STATEMENT,
        //     PACSFILE_INSERT_TUPLE_LENGTH,
        //     count,
        // );
        // let query = files.iter().fold(
        //     sqlx::query(&statement),
        //     |query, file| {
        //         // N.B. order must exactly match PACSFILE_INSERT_STATEMENT
        //         query
        //             .bind(creation_date)
        //             .bind(&file.path)
        //             .bind(&file.PatientID)
        //             .bind(file.PatientName.as_ref())
        //             .bind(&file.StudyInstanceUID)
        //             .bind(file.StudyDescription.as_ref())
        //             .bind(&file.SeriesInstanceUID)
        //             .bind(file.SeriesDescription.as_ref())
        //             .bind(file.pacs_name.as_str())
        //             .bind(file.PatientName.as_ref())
        //             .bind(file.PatientBirthDate.as_ref())
        //             .bind(file.PatientSex.as_ref())
        //             .bind(file.Modality.as_ref())
        //             .bind(file.ProtocolName.as_ref())
        //             .bind(&file.StudyDate)
        //             .bind(file.AccessionNumber.as_ref())
        //     }
        // );
        // let result = query.execute(&mut **transaction).await?;
        // Ok(Some((count, result.rows_affected())))
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
    let (unregistered_files, already_registered_paths) =
        separate_existing(files, &currently_registered, |f| f.path.as_str());
    report_already_registered_files_via_opentelemetry(&already_registered_paths).await;
    Ok(unregistered_files)
}

/// Map elements of `x` using `key_fn` and return:
///
/// - elements of `x` not found in `y`
/// - elements of `x` found in `y`
fn separate_existing<'a, 'b, T, S: AsRef<str>, F>(
    x: &'a [T],
    y: &'b [S],
    key_fn: F,
) -> (Vec<&'a T>, Vec<&'a str>)
where
    F: Fn(&T) -> &str,
{
    let existing_items: Vec<&str> = y.iter().map(|s| s.as_ref()).collect();
    let already_registered: Vec<&str> = x
        .iter()
        .map(|item| key_fn(item))
        .filter(|item| existing_items.contains(item))
        .collect();
    let remaining_items = x
        .iter()
        .filter(|item| !already_registered.contains(&key_fn(item)))
        .collect();
    (remaining_items, already_registered)
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

/// Query the database for fnames which may already exist.
async fn query_for_existing(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    files: &[PacsFileRegistrationRequest],
) -> Result<Vec<String>, sqlx::Error> {
    if let Some(n_files) = NonZeroUsize::new(files.len()) {
        let paths: Vec<_> = files.iter().map(|file| file.path.to_string()).collect();
        let query = sqlx::query_scalar!(
            "SELECT fname FROM pacsfiles_pacsfile INNER JOIN UNNEST($1::text[]) AS incoming_paths ON fname = incoming_paths WHERE fname = incoming_paths",
            &paths
        );
        query.fetch_all(&mut **transaction).await
    } else {
        Ok(Vec::with_capacity(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicomrs_options::ClientAETitle;
    use rstest::*;
    use sqlx::postgres::PgPoolOptions;
    use std::collections::HashSet;

    #[fixture]
    #[once]
    fn pool() -> sqlx::PgPool {
        futures::executor::block_on(async {
            let pool = PgPoolOptions::new()
                .max_connections(4)
                .connect(env!("DATABASE_URL"))
                .await
                .unwrap();
            add_example_data(&pool).await.unwrap();
            pool
        })
    }

    async fn add_example_data(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO pacsfiles_pacs (identifier) VALUES ('OXIUNITTEST') ON CONFLICT DO NOTHING"
        )
        .execute(pool)
        .await?;
        sqlx::query!(
            r#"MERGE INTO pacsfiles_pacsfile pacsfile USING (
                SELECT *, (SELECT id FROM pacsfiles_pacs WHERE identifier = 'OXIUNITTEST') as pacs_id FROM (
                    VALUES
                    ('2024-05-07 19:32:11.000001+00'::timestamptz, 'SERVICES/PACS/OXIUNITTEST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0183-1.3.12.2.1107.5.2.19.45152.2013030808105561901985453.dcm', '1449c1d',   'Anon Pienaar', '1.2.840.113845.11.1000000001785349915.20130308061609.6346698', 'MR-Brain w/o Contrast', '1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0', 'SAG MPRAGE 220 FOV',  1096,         '2009-07-01'::date, 'M',          'MR',       'SAG MPRAGE 220 FOV',  '2013-03-08'::date, '98edede8b2'),
                    ('2024-05-07 19:31:25.080211+00'::timestamptz, 'SERVICES/PACS/OXIUNITTEST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0184-1.3.12.2.1107.5.2.19.45152.2013030808105562925785459.dcm', '1449c1d',   'Anon Pienaar', '1.2.840.113845.11.1000000001785349915.20130308061609.6346698', 'MR-Brain w/o Contrast', '1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0', 'SAG MPRAGE 220 FOV',  1096,         '2009-07-01'::date, 'M',          'MR',       'SAG MPRAGE 220 FOV',  '2013-03-08'::date, '98edede8b2'),
                    ('2024-05-07 19:32:11.000001+00'::timestamptz, 'SERVICES/PACS/OXIUNITTEST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0185-1.3.12.2.1107.5.2.19.45152.2013030808105550546785443.dcm', '1449c1d',   'Anon Pienaar', '1.2.840.113845.11.1000000001785349915.20130308061609.6346698', 'MR-Brain w/o Contrast', '1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0', 'SAG MPRAGE 220 FOV',  1096,         '2009-07-01'::date, 'M',          'MR',       'SAG MPRAGE 220 FOV',  '2013-03-08'::date, '98edede8b2')
                ) AS Examples(creation_date,                       fname,                                                                                                                                                                                            "PatientID", "PatientName",  "StudyInstanceUID",                                             "StudyDescription",      "SeriesInstanceUID",                                          "SeriesDescription",   "PatientAge", "PatientBirthDate", "PatientSex", "Modality", "ProtocolName",        "StudyDate",        "AccessionNumber")
            ) examples
            ON pacsfile.fname = examples.fname
            WHEN NOT MATCHED THEN
                INSERT (creation_date, fname, "PatientID", "PatientName", "StudyInstanceUID", "StudyDescription", "SeriesInstanceUID", "SeriesDescription", pacs_id, "PatientAge", "PatientBirthDate", "PatientSex", "Modality", "ProtocolName", "StudyDate", "AccessionNumber")
                VALUES (examples.creation_date, examples.fname, examples."PatientID", examples."PatientName", examples."StudyInstanceUID", examples."StudyDescription", examples."SeriesInstanceUID", examples."SeriesDescription", examples.pacs_id, examples."PatientAge", examples."PatientBirthDate", examples."PatientSex", examples."Modality", examples."ProtocolName", examples."StudyDate", examples."AccessionNumber")
            WHEN MATCHED THEN
                UPDATE SET
                    creation_date = examples.creation_date,
                    fname = examples.fname,
                    "PatientID" = examples."PatientID",
                    "StudyInstanceUID" = examples."StudyInstanceUID",
                    "StudyDescription" = examples."StudyDescription",
                    "SeriesInstanceUID" = examples."SeriesInstanceUID",
                    "SeriesDescription" = examples."SeriesDescription",
                    pacs_id = examples.pacs_id,
                    "PatientAge" = examples."PatientAge",
                    "PatientBirthDate" = examples."PatientBirthDate",
                    "PatientSex" = examples."PatientSex",
                    "Modality" = examples."Modality",
                    "ProtocolName" = examples."ProtocolName",
                    "StudyDate" = examples."StudyDate",
                    "AccessionNumber" = examples."AccessionNumber"
            "#
        ).execute(pool).await?;
        Ok(())
    }

    #[fixture]
    fn example_requests() -> Vec<PacsFileRegistrationRequest> {
        vec![
            PacsFileRegistrationRequest {
                path: "SERVICES/PACS/OXIUNITTEST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0184-1.3.12.2.1107.5.2.19.45152.2013030808105562925785459.dcm".to_string(),
                PatientID: "1449c1d".to_string(),
                StudyDate: "2013-03-08".to_string(),
                StudyInstanceUID: "1.2.840.113845.11.1000000001785349915.20130308061609.6346698".to_string(),
                SeriesInstanceUID: "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0".to_string(),
                pacs_name: ClientAETitle::from_static("OXIUNITTEST"),
                PatientName: Some("Anon Pienaar".to_string()),
                PatientBirthDate: Some("2009-07-01".to_string()),
                PatientAge: Some(1096),
                PatientSex: Some("M".to_string()),
                AccessionNumber: Some("98edede8b2".to_string()),
                Modality: Some("MR".to_string()),
                ProtocolName: Some("SAG MPRAGE 220 FOV".to_string()),
                StudyDescription: Some("MR-Brain w/o Contrast".to_string()),
                SeriesDescription: Some("SAG MPRAGE 220 FOV".to_string()),
            },
            PacsFileRegistrationRequest {
                path: "SERVICES/PACS/OXIUNITTEST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0185-1.3.12.2.1107.5.2.19.45152.2013030808105550546785443.dcm".to_string(),
                PatientID: "1449c1d".to_string(),
                StudyDate: "2013-03-08".to_string(),
                StudyInstanceUID: "1.2.840.113845.11.1000000001785349915.20130308061609.6346698".to_string(),
                SeriesInstanceUID: "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0".to_string(),
                pacs_name: ClientAETitle::from_static("OXIUNITTEST"),
                PatientName: Some("Anon Pienaar".to_string()),
                PatientBirthDate: Some("2009-07-01".to_string()),
                PatientAge: Some(1096),
                PatientSex: Some("M".to_string()),
                AccessionNumber: Some("98edede8b2".to_string()),
                Modality: Some("MR".to_string()),
                ProtocolName: Some("SAG MPRAGE 220 FOV".to_string()),
                StudyDescription: Some("MR-Brain w/o Contrast".to_string()),
                SeriesDescription: Some("SAG MPRAGE 220 FOV".to_string()),
            },
            PacsFileRegistrationRequest {
                path: "SERVICES/PACS/OXIUNITTEST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0186-1.3.12.2.1107.5.2.19.45152.2013030808105578565885477.dcm".to_string(),
                PatientID: "1449c1d".to_string(),
                StudyDate: "2013-03-08".to_string(),
                StudyInstanceUID: "1.2.840.113845.11.1000000001785349915.20130308061609.6346698".to_string(),
                SeriesInstanceUID: "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0".to_string(),
                pacs_name: ClientAETitle::from_static("OXIUNITTEST"),
                PatientName: Some("Anon Pienaar".to_string()),
                PatientBirthDate: Some("2009-07-01".to_string()),
                PatientAge: Some(1096),
                PatientSex: Some("M".to_string()),
                AccessionNumber: Some("98edede8b2".to_string()),
                Modality: Some("MR".to_string()),
                ProtocolName: Some("SAG MPRAGE 220 FOV".to_string()),
                StudyDescription: Some("MR-Brain w/o Contrast".to_string()),
                SeriesDescription: Some("SAG MPRAGE 220 FOV".to_string()),
            },
        ]
    }

    #[rstest]
    fn test_split_existing() {
        let x = ["a", "b", "c", "d", "e"];
        let y = ["b", "d", "e", "f", "g"];
        let union = vec!["a", "c"];
        let only_in_y = vec!["b", "d", "e"];
        let expected = (union.iter().collect(), only_in_y);
        let actual = separate_existing(&x, &y, |s| s);
        assert_eq!(expected, actual)
    }

    #[rstest]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_query_for_existing(
        pool: &sqlx::PgPool,
        example_requests: Vec<PacsFileRegistrationRequest>,
    ) -> Result<(), sqlx::Error> {
        let mut transaction = pool.begin().await?;
        let actual = query_for_existing(&mut transaction, &example_requests).await?;
        let actual_set = HashSet::from_iter(actual.iter().map(|s| s.as_str()));
        let expected_set = HashSet::from([
            "SERVICES/PACS/OXIUNITTEST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0184-1.3.12.2.1107.5.2.19.45152.2013030808105562925785459.dcm",
            "SERVICES/PACS/OXIUNITTEST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0185-1.3.12.2.1107.5.2.19.45152.2013030808105550546785443.dcm",
        ]);
        assert_eq!(actual_set, expected_set);
        Ok(())
    }
}
