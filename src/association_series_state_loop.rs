use crate::dicomrs_settings::AETitle;
use crate::enums::{AssociationEvent, SeriesEvent};
use crate::error::{DicomRequiredTagError, DicomStorageError, HandleLoopError};
use crate::pacs_file::{BadTag, PacsFileRegistration};
use crate::types::{DicomFilePath, DicomInfo, PendingDicomInstance, SeriesKey, SeriesPath};
use camino::{Utf8Path, Utf8PathBuf};
use dicom::object::DefaultDicomObject;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use ulid::Ulid;

struct Association {
    pacs_name: AETitle,
    series: HashMap<SeriesKey, DicomInfo<SeriesPath>>,
}

impl Association {
    fn new(pacs_name: AETitle) -> Self {
        Self {
            pacs_name,
            series: Default::default(),
        }
    }
}

type InflightAssociations = HashMap<Ulid, Association>;

/// Stateful handling of [AssociationEvent].
///
/// - On every DICOM instance received, read its metadata (such as PatientName, Modality, ...),
///   and create a (tokio) task in which the data is written to storage as a DICOM file.
/// - At the end of every association, send a [SeriesEvent::Finish] for each series we saw
///   during the association.
pub(crate) async fn association_series_state_loop(
    mut receiver: UnboundedReceiver<AssociationEvent>,
    sender: UnboundedSender<(SeriesKey, PendingDicomInstance)>,
    files_root: Utf8PathBuf,
) -> Result<Result<(), HandleLoopError>, SendError<(SeriesKey, PendingDicomInstance)>> {
    let mut inflight_associations: InflightAssociations = Default::default();
    let mut everything_ok = true;
    let files_root = Arc::new(files_root);
    while let Some(event) = receiver.recv().await {
        match match_event(event, &mut inflight_associations, &files_root) {
            Ok(messages) => {
                for message in messages {
                    sender.send(message)?
                }
            }
            Err(_) => {
                everything_ok = false;
            }
        }
    }
    let result = if everything_ok {
        Ok(())
    } else {
        Err(HandleLoopError(
            "There was an error processing DICOM objects.",
        ))
    };
    Ok(result)
}

/// Helper function which handles most of what [association_series_state_loop] is supposed to do.
///
/// Since this function is not async, it helps to protect the invariant that
/// [SeriesEvent::Finish] will be the last sent message of a series (there is no async
/// code to cause a race condition).
fn match_event(
    event: AssociationEvent,
    inflight_associations: &mut InflightAssociations,
    files_root: &Arc<Utf8PathBuf>,
) -> Result<Vec<(SeriesKey, PendingDicomInstance)>, ()> {
    match event {
        AssociationEvent::Start { ulid, aec } => {
            inflight_associations.insert(ulid, Association::new(aec));
            Ok(vec![])
        }
        AssociationEvent::DicomInstance { ulid, dcm } => {
            match receive_dicom_instance(ulid, dcm, inflight_associations, files_root) {
                Ok((series, task)) => Ok(vec![(series, SeriesEvent::Instance(task))]),
                Err(e) => {
                    tracing::error!(association_ulid = ulid.to_string(), message = e.to_string());
                    Err(())
                }
            }
        }
        AssociationEvent::Finish { ulid, .. } => {
            let association = inflight_associations
                .remove(&ulid)
                .expect("Unknown association ULID");
            Ok(finish_association(association.series))
        }
    }
}

/// Receive a DICOM instance. It will be taken note of in `inflight_associations`.
///
/// For every DICOM instance received: create a task to store the DICOM instance as a file.
/// When the task finishes, it returns the count of files stored.
///
/// The tasks are returned.
fn receive_dicom_instance(
    ulid: Ulid,
    dcm: DefaultDicomObject,
    inflight_associations: &mut InflightAssociations,
    files_root: &Arc<Utf8PathBuf>,
) -> Result<(SeriesKey, JoinHandle<Result<(), DicomStorageError>>), DicomRequiredTagError> {
    let association = inflight_associations
        .get_mut(&ulid)
        .expect("Unknown association ULID");
    let pacs_name = association.pacs_name.clone();
    let (pacs_file, bad_tags) = PacsFileRegistration::new(pacs_name, dcm)?;
    report_bad_tags(&pacs_file.data, ulid, bad_tags);
    let series_key = SeriesKey::new(
        pacs_file.data.SeriesInstanceUID.clone(),
        pacs_file.data.pacs_name.clone(),
    );
    association
        .series
        .entry(series_key.clone())
        .or_insert_with(|| pacs_file.data.clone().into());
    let storage_task = {
        let files_root = Arc::clone(files_root);
        tokio::task::spawn_blocking(move || write_dicom_wotel(&files_root, &pacs_file))
    };
    Ok((series_key, storage_task))
}

/// Creates messages for the end of an association.
fn finish_association(
    series_counts: HashMap<SeriesKey, DicomInfo<SeriesPath>>,
) -> Vec<(SeriesKey, PendingDicomInstance)> {
    series_counts
        .into_iter()
        .map(|(s, c)| (s, SeriesEvent::Finish(c)))
        .collect()
}

/// Wraps [write_dicom] with OpenTelemetry logging.
fn write_dicom_wotel(
    files_root: &Utf8Path,
    pacs_file: &PacsFileRegistration,
) -> Result<(), DicomStorageError> {
    match write_dicom(pacs_file, files_root) {
        Ok(path) => tracing::info!(event = "storage", path = path.into_string()),
        Err(e) => {
            tracing::error!(event = "storage", error = e.to_string());
            return Err(e);
        }
    }
    Ok(())
}

/// Write a DICOM object to the filesystem.
fn write_dicom<P: AsRef<Utf8Path>>(
    pacs_file: &PacsFileRegistration,
    files_root: P,
) -> Result<Utf8PathBuf, DicomStorageError> {
    let output_path = files_root.as_ref().join(pacs_file.data.path.as_str());
    if let Some(parent_dir) = output_path.parent() {
        fs_err::create_dir_all(parent_dir)?;
    }
    pacs_file.obj.write_to_file(&output_path)?;
    Ok(output_path)
}

/// Report bad tags via OpenTelemetry.
fn report_bad_tags<T: AsRef<[BadTag]>>(
    pacs_file: &DicomInfo<DicomFilePath>,
    ulid: Ulid,
    bad_tags: T,
) {
    let bad_tags_slice = bad_tags.as_ref();
    if bad_tags_slice.is_empty() {
        return;
    }
    let bad_tags_csv = bad_tags_slice
        .iter()
        .map(|bt| bt.to_string())
        .collect::<Vec<_>>()
        .join(",");
    tracing::warn!(
        association_ulid = ulid.to_string(),
        path = pacs_file.path.as_str(),
        bad_tags = bad_tags_csv
    )
}
