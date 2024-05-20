use std::collections::{HashMap, HashSet};

use opentelemetry::trace::TraceContextExt;
use opentelemetry::{Array, Context, KeyValue, StringValue, Value};
use sqlx::postgres::PgQueryResult;
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
        count: u64,
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
        &self,
        files: &[PacsFileRegistrationRequest],
    ) -> Result<(), PacsFileDatabaseError> {
        let mut transaction = self.pool.begin().await?;
        let unregistered_files =
            warn_and_remove_already_registered(&mut transaction, files).await?;
        let (count, rows_affected) =
            insert_into_pacsfile(&mut transaction, unregistered_files, self.get_now()).await?;
        if count == rows_affected {
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
) -> Result<(u64, u64), sqlx::Error> {
    if files.is_empty() {
        return Ok((0, 0));
    }
    create_pacs_as_needed(transaction, files.clone()).await?;
    // bulk insert with PostgreSQL example:
    // https://github.com/launchbadge/sqlx/blob/main/FAQ.md#how-can-i-bind-an-array-to-a-values-clause-how-can-i-do-bulk-inserts
    let query = sqlx::query!(
        r#"INSERT INTO pacsfiles_pacsfile (
                   creation_date,      fname,     "PatientID", "PatientName", "StudyInstanceUID", "StudyDescription", "SeriesInstanceUID", "SeriesDescription", "PatientAge",  "PatientBirthDate", "PatientSex", "Modality", "ProtocolName", "StudyDate", "AccessionNumber", pacs_id
        )
        SELECT
                   creation_date,      fname,     "PatientID", "PatientName", "StudyInstanceUID", "StudyDescription", "SeriesInstanceUID", "SeriesDescription", "PatientAge",  "PatientBirthDate", "PatientSex", "Modality", "ProtocolName", "StudyDate", "AccessionNumber", pacs.id
        FROM
            UNNEST($1::timestamptz[], $2::text[], $3::text[],  $4::text[],    $5::text[],         $6::text[],         $7::text[],          $8::text[],          $9::integer[], $10::date[],        $11::text[],  $12::text[], $13::text[],   $14::date[], $15::text[],       $16::text[])
            AS incoming(creation_date, fname,     "PatientID", "PatientName", "StudyInstanceUID", "StudyDescription", "SeriesInstanceUID", "SeriesDescription", "PatientAge",  "PatientBirthDate", "PatientSex", "Modality", "ProtocolName", "StudyDate", "AccessionNumber", pacs_name)
        LEFT JOIN pacsfiles_pacs pacs ON incoming.pacs_name = pacs.identifier
        "#,
        &files.iter().map(|_| creation_date).collect::<Vec<_>>(),
        &files.iter().map(|f| f.path.to_string()).collect::<Vec<_>>(),
        &files
            .iter()
            .map(|f| f.PatientID.to_string())
            .collect::<Vec<_>>(),
        &files
            .iter()
            .map(|f| f.PatientName.clone())
            .collect::<Vec<_>>() as &[Option<String>],
        &files
            .iter()
            .map(|f| f.StudyInstanceUID.to_string())
            .collect::<Vec<_>>(),
        &files
            .iter()
            .map(|f| f.StudyDescription.clone())
            .collect::<Vec<_>>() as &[Option<String>],
        &files
            .iter()
            .map(|f| f.SeriesInstanceUID.to_string())
            .collect::<Vec<_>>(),
        &files
            .iter()
            .map(|f| f.SeriesDescription.clone())
            .collect::<Vec<_>>() as &[Option<String>],
        &files
            .iter()
            .map(|f| f.PatientAge.clone())
            .collect::<Vec<_>>() as &[Option<i32>],
        &files
            .iter()
            .map(|f| f.PatientBirthDate.clone())
            .collect::<Vec<_>>() as &[Option<String>],
        &files
            .iter()
            .map(|f| f.PatientSex.clone())
            .collect::<Vec<_>>() as &[Option<String>],
        &files.iter().map(|f| f.Modality.clone()).collect::<Vec<_>>() as &[Option<String>],
        &files
            .iter()
            .map(|f| f.ProtocolName.clone())
            .collect::<Vec<_>>() as &[Option<String>],
        &files.iter().map(|f| f.StudyDate).collect::<Vec<_>>(),
        &files
            .iter()
            .map(|f| f.AccessionNumber.clone())
            .collect::<Vec<_>>() as &[Option<String>],
        &files
            .iter()
            .map(|f| f.pacs_name.to_string())
            .collect::<Vec<_>>()
    );
    let result = query.execute(&mut **transaction).await?;
    Ok((files.len() as u64, result.rows_affected()))
}

async fn create_pacs_as_needed(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    files: impl IntoIterator<Item = &PacsFileRegistrationRequest>,
) -> Result<PgQueryResult, sqlx::Error> {
    let unique_pacs_names: Vec<String> = files
        .into_iter()
        .map(|f| f.pacs_name.as_str())
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|pacs_name| pacs_name.to_string())
        .collect();
    sqlx::query!(
        r#"INSERT INTO pacsfiles_pacs(identifier)
        SELECT new_names FROM UNNEST($1::text[]) AS new_names
        LEFT JOIN pacsfiles_pacs ON new_names = pacsfiles_pacs.identifier
        WHERE pacsfiles_pacs.id IS NULL"#,
        &unique_pacs_names
    )
    .execute(&mut **transaction)
    .await
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
    if files.is_empty() {
        return Ok(Vec::with_capacity(0));
    }
    let paths: Vec<_> = files.iter().map(|file| file.path.to_string()).collect();
    let query = sqlx::query_scalar!(
            "SELECT fname FROM pacsfiles_pacsfile INNER JOIN UNNEST($1::text[]) AS incoming_paths ON fname = incoming_paths WHERE fname = incoming_paths",
            &paths
        );
    query.fetch_all(&mut **transaction).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicomrs_options::ClientAETitle;
    use chris::search::GetOnlyError;
    use chris::{
        types::{CubeUrl, Username},
        ChrisClient,
    };
    use futures::prelude::*;
    use rstest::*;
    use sqlx::postgres::PgPoolOptions;
    use std::collections::HashSet;
    use std::path::PathBuf;

    #[fixture]
    #[once]
    fn pool() -> sqlx::PgPool {
        futures::executor::block_on(async {
            PgPoolOptions::new()
                .max_connections(4)
                .connect(env!("DATABASE_URL"))
                .await
                .unwrap()
        })
    }

    #[fixture]
    #[once]
    fn chris_client() -> ChrisClient {
        futures::executor::block_on(async {
            let cube_url = CubeUrl::new(envmnt::get_or_panic("CHRIS_URL")).unwrap();
            let username = Username::new(envmnt::get_or_panic("CHRIS_USERNAME"));
            let password = envmnt::get_or_panic("CHRIS_PASSWORD");
            let account = chris::Account::new(&cube_url, &username, &password);
            let token = account.get_token().await.unwrap();
            ChrisClient::build(cube_url, username, token)
                .unwrap()
                .connect()
                .await
                .unwrap()
        })
    }

    async fn add_3_existing_rows(pool: &sqlx::PgPool, pacs_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO pacsfiles_pacs (identifier) VALUES ($1) ON CONFLICT DO NOTHING",
            pacs_name
        )
        .execute(pool)
        .await?;
        sqlx::query!(
            r#"MERGE INTO pacsfiles_pacsfile pacsfile USING (
                SELECT *, (SELECT id FROM pacsfiles_pacs WHERE identifier = $1) as pacs_id FROM (
                    VALUES
                    ('2024-05-07 19:32:11.000001+00'::timestamptz, $2,    '1449c1d',   'Anon Pienaar', '1.2.840.113845.11.1000000001785349915.20130308061609.6346698', 'MR-Brain w/o Contrast', '1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0', 'SAG MPRAGE 220 FOV',  1096,         '2009-07-01'::date, 'M',          'MR',       'SAG MPRAGE 220 FOV',  '2013-03-08'::date, '98edede8b2'),
                    ('2024-05-07 19:31:25.080211+00'::timestamptz, $3,    '1449c1d',   'Anon Pienaar', '1.2.840.113845.11.1000000001785349915.20130308061609.6346698', 'MR-Brain w/o Contrast', '1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0', 'SAG MPRAGE 220 FOV',  1096,         '2009-07-01'::date, 'M',          'MR',       'SAG MPRAGE 220 FOV',  '2013-03-08'::date, '98edede8b2'),
                    ('2024-05-07 19:32:11.000001+00'::timestamptz, $4,    '1449c1d',   'Anon Pienaar', '1.2.840.113845.11.1000000001785349915.20130308061609.6346698', 'MR-Brain w/o Contrast', '1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0', 'SAG MPRAGE 220 FOV',  1096,         '2009-07-01'::date, 'M',          'MR',       'SAG MPRAGE 220 FOV',  '2013-03-08'::date, '98edede8b2')
                ) AS Examples(creation_date,                       fname, "PatientID", "PatientName",  "StudyInstanceUID",                                             "StudyDescription",      "SeriesInstanceUID",                                          "SeriesDescription",   "PatientAge", "PatientBirthDate", "PatientSex", "Modality", "ProtocolName",        "StudyDate",        "AccessionNumber")
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
            "#,
            pacs_name,
            format!("SERVICES/PACS/{pacs_name}/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0183-1.3.12.2.1107.5.2.19.45152.2013030808105561901985453.dcm"),
            format!("SERVICES/PACS/{pacs_name}/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0184-1.3.12.2.1107.5.2.19.45152.2013030808105562925785459.dcm"),
            format!("SERVICES/PACS/{pacs_name}/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0185-1.3.12.2.1107.5.2.19.45152.2013030808105550546785443.dcm")
        ).execute(pool).await?;
        Ok(())
    }

    fn example_requests(pacs_name: &str) -> Vec<PacsFileRegistrationRequest> {
        vec![
            PacsFileRegistrationRequest {
                path: format!("SERVICES/PACS/{pacs_name}/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0184-1.3.12.2.1107.5.2.19.45152.2013030808105562925785459.dcm"),
                PatientID: "1449c1d".to_string(),
                StudyDate: time::macros::date!(2013-03-08),
                StudyInstanceUID: "1.2.840.113845.11.1000000001785349915.20130308061609.6346698".to_string(),
                SeriesInstanceUID: "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0".to_string(),
                pacs_name: pacs_name.into(),
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
                path: format!("SERVICES/PACS/{pacs_name}/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0185-1.3.12.2.1107.5.2.19.45152.2013030808105550546785443.dcm"),
                PatientID: "1449c1d".to_string(),
                StudyDate: time::macros::date!(2013-03-08),
                StudyInstanceUID: "1.2.840.113845.11.1000000001785349915.20130308061609.6346698".to_string(),
                SeriesInstanceUID: "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0".to_string(),
                pacs_name: pacs_name.into(),
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
                path: format!("SERVICES/PACS/{pacs_name}/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0186-1.3.12.2.1107.5.2.19.45152.2013030808105578565885477.dcm"),
                PatientID: "1449c1d".to_string(),
                StudyDate: time::macros::date!(2013-03-08),
                StudyInstanceUID: "1.2.840.113845.11.1000000001785349915.20130308061609.6346698".to_string(),
                SeriesInstanceUID: "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0".to_string(),
                pacs_name: pacs_name.into(),
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
    async fn test_query_for_existing(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        let mut transaction = pool.begin().await?;
        add_3_existing_rows(pool, "OUT_QUERY_FOR_EXIST").await?;
        let example_requests = example_requests("OUT_QUERY_FOR_EXIST");
        let actual = query_for_existing(&mut transaction, &example_requests).await?;
        let actual_set = HashSet::from_iter(actual.iter().map(|s| s.as_str()));
        let expected_set = HashSet::from([
            "SERVICES/PACS/OUT_QUERY_FOR_EXIST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0184-1.3.12.2.1107.5.2.19.45152.2013030808105562925785459.dcm",
            "SERVICES/PACS/OUT_QUERY_FOR_EXIST/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06/0185-1.3.12.2.1107.5.2.19.45152.2013030808105550546785443.dcm",
        ]);
        assert_eq!(actual_set, expected_set);
        Ok(())
    }

    #[rstest]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_register(
        pool: &sqlx::PgPool,
        chris_client: &ChrisClient,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db_client = CubePostgresClient::new(pool.clone(), Some(time::macros::offset!(-5)));
        let pacs_name = format!("OUT_{}", time::OffsetDateTime::now_utc().unix_timestamp());
        let requests = example_requests(&pacs_name);

        // sanity check: are we starting from a fresh state?
        let count = chris_client
            .pacsfiles()
            .pacs_identifier(&pacs_name)
            .search()
            .get_count()
            .await?;
        assert_eq!(count, 0);

        // register files
        pretend_to_receive_dicom_files(&requests).await?;
        db_client.register(&requests).await?;

        // assert files were registered
        let count = chris_client
            .pacsfiles()
            .pacs_identifier(&pacs_name)
            .search()
            .get_count()
            .await?;
        assert_eq!(count, requests.len());
        let pacs_name_ptr = &pacs_name;
        futures::stream::iter(requests.iter())
            .map(Ok)
            .try_for_each_concurrent(4, |req| async move {
                let file = chris_client
                    .pacsfiles()
                    .fname_exact(&req.path)
                    .search()
                    .get_only()
                    .await?;
                assert_eq!(file.object.fname.as_str(), &req.path);
                assert_eq!(&file.object.patient_id, &req.PatientID);
                assert_eq!(&file.object.pacs_identifier, pacs_name_ptr);
                Ok::<_, GetOnlyError>(())
            })
            .await?;

        // assert reregistering files should be idempotent
        db_client.register(&requests).await?;
        let count = chris_client
            .pacsfiles()
            .pacs_identifier(&pacs_name)
            .search()
            .get_count()
            .await?;
        assert_eq!(count, requests.len());
        Ok(())
    }

    async fn pretend_to_receive_dicom_files(
        requests: impl IntoIterator<Item = &PacsFileRegistrationRequest>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::var("CHRIS_FILES_ROOT")
            .map(PathBuf::from)
            .expect("The environment variable CHRIS_FILES_ROOT must be set.");
        futures::stream::iter(requests.into_iter())
            .map(|req| root.join(&req.path))
            .map(Ok)
            .try_for_each_concurrent(4, |p| async move {
                if let Some(dir) = p.parent() {
                    fs_err::tokio::create_dir_all(dir).await?;
                }
                fs_err::tokio::write(p, b"i am written by pretend_to_receive_dicom_files").await
            })
            .await
            .map_err(|e| e.into())
    }
}
