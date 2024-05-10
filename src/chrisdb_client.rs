use crate::dicomrs_options::ClientAETitle;
use crate::pacs_file::PacsFileRegistrationRequest;
use itertools::Itertools;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use time::OffsetDateTime;

/// A client which writes to The _ChRIS_ backend's PostgreSQL database.
pub(crate) struct CubePostgresClient {
    /// PostgreSQL database client
    client: postgres::Client,
    /// The pacsfiles_pacs table, which maps string PACS names to integer IDs
    pacs: HashMap<String, u32>,
    /// Timezone for the "creation_date" field.
    tz: Option<time::UtcOffset>,
}

impl CubePostgresClient {
    pub fn new(client: postgres::Client, tz: Option<time::UtcOffset>) -> Self {
        Self {
            client,
            pacs: Default::default(),
            tz,
        }
    }

    /// Register DICOM file metadata to CUBE's database.
    pub fn register(
        &mut self,
        files: &[PacsFileRegistrationRequest],
    ) -> Result<u64, postgres::Error> {
        // TODO remove duplicates in a transaction
        todo!()
    }

    fn get_now(&self) -> time::OffsetDateTime {
        let now = time::OffsetDateTime::now_utc();
        if let Some(offset) = self.tz {
            now.checked_to_offset(offset).unwrap_or(now)
        } else {
            now
        }
    }
}

const INSERT_STATEMENT: &str = r#"INSERT INTO pacsfiles_pacsfile (
creation_date, fname, "PatientID", "PatientName", "StudyInstanceUID", "StudyDescription",
"SeriesInstanceUID", "SeriesDescription", pacs_id, "PatientAge", "PatientBirthDate",
"PatientSex", "Modality", "ProtocolName", "StudyDate", "AccessionNumber") VALUES"#;
const TUPLE_LENGTH: usize = 16;

struct PacsfileRegistrationTransaction<'a> {
    transaction: postgres::Transaction<'a>,
    pacs_id_statement: postgres::Statement,
}

impl<'a> PacsfileRegistrationTransaction<'a> {
    fn new(client: &'a mut postgres::Client) -> Result<Self, postgres::Error> {
        let mut transaction = client.transaction()?;
        let pacs_id_statement =
            transaction.prepare("SELECT id FROM pacsfiles_pacs WHERE identifier = $1")?;
        Ok(Self {
            transaction,
            pacs_id_statement,
        })
    }

    fn remove_duplicates<'b>(
        &'a self,
        files: &'b [PacsFileRegistrationRequest],
    ) -> Result<Vec<&'b PacsFileRegistrationRequest>, postgres::Error> {
        todo!()
    }

    fn prepare_params<'b>(
        &'a mut self,
        creation_date: OffsetDateTime,
        files: &'b [PacsFileRegistrationRequest],
    ) -> Result<Vec<Box<dyn postgres::types::ToSql + Sync + 'b>>, postgres::Error> {
        let mut params: Vec<Box<dyn postgres::types::ToSql + Sync>> =
            Vec::with_capacity(files.len() * TUPLE_LENGTH);
        for file in files {
            // order must be as specified in INSERT_STATEMENT
            params.push(Box::new(creation_date));
            params.push(Box::new(&file.path)); // fname
            params.push(Box::new(&file.PatientID));
            params.push(Box::new(&file.PatientName));
            params.push(Box::new(&file.StudyInstanceUID));
            params.push(Box::new(&file.StudyDescription));
            params.push(Box::new(&file.SeriesInstanceUID));
            params.push(Box::new(&file.SeriesDescription));
            params.push(Box::new(self.get_pacs_id_of(&file.pacs_name)?));
            params.push(Box::new(&file.PatientAge));
            params.push(Box::new(&file.PatientBirthDate));
            params.push(Box::new(&file.PatientSex));
            params.push(Box::new(&file.Modality));
            params.push(Box::new(&file.ProtocolName));
            params.push(Box::new(&file.StudyDate));
            params.push(Box::new(&file.AccessionNumber));
        }
        Ok(params)
    }

    fn get_pacs_id_of(&mut self, aec: &ClientAETitle) -> Result<u32, postgres::Error> {
        let res = self
            .transaction
            .query_opt(&self.pacs_id_statement, &[&aec.as_str()])?;
        if let Some(row) = res {
            return Ok(row.get(0));
        }
        self.transaction.execute(
            "INSERT INTO pacsfiles_pacs (identifier) VALUES ($1)",
            &[&aec.as_str()],
        )?;
        self.transaction
            .query_one(&self.pacs_id_statement, &[&aec.as_str()])
            .map(|row| row.get(0))
    }
}

fn statement_for(len: NonZeroUsize) -> String {
    let tuples = (0..len.get())
        .map(|n| {
            let start = n * TUPLE_LENGTH + 1;
            let end = start + TUPLE_LENGTH;
            let placeholders = (start..end).map(|i| format!("${i}")).join(",");
            format!("({placeholders})")
        })
        .join(" ");
    format!("{INSERT_STATEMENT} {tuples}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statement_for1() {
        let expected1 = format!(
            "{} ({})",
            INSERT_STATEMENT,
            (1..=16).map(|i| format!("${i}")).join(",")
        );
        let actual1 = statement_for(NonZeroUsize::new(1).unwrap());
        assert_eq!(actual1, expected1);
    }

    #[test]
    fn test_statement_for3() {
        let expected1 = format!(
            "{} ({}) ({}) ({})",
            INSERT_STATEMENT,
            (1..=16).map(|i| format!("${i}")).join(","),
            (17..=32).map(|i| format!("${i}")).join(","),
            (33..=48).map(|i| format!("${i}")).join(","),
        );
        let actual1 = statement_for(NonZeroUsize::new(3).unwrap());
        assert_eq!(actual1, expected1);
    }
}
