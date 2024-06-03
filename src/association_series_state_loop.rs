//! Functionality related to tracking the state of series being received
//! and writing DICOM objects to files.
use crate::dicomrs_settings::{ClientAETitle, OurAETitle};
use crate::enums::{AssociationEvent, PendingRegistration};
use crate::error::{DicomRequiredTagError, DicomStorageError, HandleLoopError};
use crate::findscu::FindScuParameters;
use crate::pacs_file::{BadTag, PacsFileRegistration, PacsFileRegistrationRequest};
use crate::series_key_set::SeriesKeySet;
use camino::{Utf8Path, Utf8PathBuf};
use dicom::object::DefaultDicomObject;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use ulid::Ulid;

/// Stateful handling of [AssociationEvent].
///
/// Most importantly, it writes received DICOM instances to files in storage.
/// It also handles the creation of "Oxidicom Custom Metadata" files
/// (`NumberOfSeriesRelatedInstances=M` and `OxidicomAttemptedPushCount=N`).
///
/// It keeps track of the series received for each association, and handles them accordingly:
///
/// - On the first instance of a new series, attempt to contact the PACS the instance was sent from,
///   and query for the `NumberOfSeriesRelatedInstances`.
/// - On every DICOM instance received, extract its metadata, and create a task to store the
///   instance as a DICOM file.
/// - At the end of every association, create all the `OxidicomAttemptedPushCount` files for each
///   series of the finished association, and finally send [PendingRegistration::End].
pub(crate) async fn association_series_state_loop(
    mut receiver: UnboundedReceiver<AssociationEvent>,
    sender: UnboundedSender<(SeriesKeySet, PendingRegistration)>,
    files_root: Utf8PathBuf,
) -> Result<Result<(), HandleLoopError>, SendError<(SeriesKeySet, PendingRegistration)>> {
    let mut inflight_associations: HashMap<Ulid, Association> = Default::default();
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
/// [PendingRegistration::End] will be the last sent message of a series (there is no async
/// code to cause a race condition).
fn match_event(
    event: AssociationEvent,
    inflight_associations: &mut HashMap<Ulid, Association>,
    files_root: &Arc<Utf8PathBuf>,
) -> Result<Vec<(SeriesKeySet, PendingRegistration)>, ()> {
    match event {
        AssociationEvent::Start {
            ulid,
            aec,
            aet,
            pacs_address,
        } => {
            if pacs_address.is_none() {
                tracing::warn!(
                    association_ulid = ulid.to_string(),
                    "OXIDICOM_PACS_ADDRESS not configured for this association."
                );
            }
            inflight_associations.insert(ulid, Association::new(aec, aet, pacs_address));
            Ok(Vec::with_capacity(0))
        }
        AssociationEvent::DicomInstance { ulid, dcm } => {
            match receive_dicom_instance(ulid, dcm, inflight_associations, &files_root) {
                Ok((series, tasks)) => {
                    let pending_tasks = tasks
                        .into_iter()
                        .map(PendingRegistration::Task)
                        .map(|task| (series.clone(), task))
                        .collect();
                    Ok(pending_tasks)
                }
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
            Ok(finish_association(ulid, association.series, &files_root))
        }
    }
}

/// Receive a DICOM instance. It will be taken note of in `inflight_associations`.
///
/// - On the first DICOM instance of a series received: try to ask the PACS server for the `NumberOfSeriesRelatedInstances`.
/// - For every DICOM instance received: create a task to store the DICOM instance as a file
///
/// The tasks are returned.
fn receive_dicom_instance(
    ulid: Ulid,
    dcm: DefaultDicomObject,
    inflight_associations: &mut HashMap<Ulid, Association>,
    files_root: &Arc<Utf8PathBuf>,
) -> Result<
    (
        SeriesKeySet,
        Vec<JoinHandle<Result<PacsFileRegistrationRequest, ()>>>,
    ),
    DicomRequiredTagError,
> {
    let association = inflight_associations
        .get_mut(&ulid)
        .expect("Unknown association ULID");
    let pacs_name = association.aec.clone();
    let (pacs_file, bad_tags) = PacsFileRegistration::new(pacs_name, dcm)?;
    report_bad_tags(&pacs_file.request, ulid, bad_tags);
    let series_key_set = SeriesKeySet::from(pacs_file.request.clone());
    let storage_task = {
        let files_root = Arc::clone(files_root);
        tokio::task::spawn_blocking(move || {
            store_dicom(&files_root, &pacs_file).map(|_| pacs_file.request)
        })
    };
    let tasks =
        if let Some(findscu_params) = maybe_findscu(ulid, series_key_set.clone(), association) {
            let series_key_set = series_key_set.clone();
            let files_root = Arc::clone(files_root);
            let findscu_tasks = tokio::task::spawn_blocking(move || {
                findscu_params
                    .get_number_of_series_related_instances()
                    .map(|n| {
                        series_key_set.into_oxidicom_custom_pacsfile(
                            ulid,
                            "NumberOfSeriesRelatedInstances",
                            n.to_string(),
                        )
                    })
                    .and_then(|pacs_file| create_blank_file(&files_root, pacs_file))
            });
            vec![storage_task, findscu_tasks]
        } else {
            vec![storage_task]
        };
    Ok((series_key_set, tasks))
}

/// Creates messages for the end of an association.
///
/// For each series with one or more instance:
///
/// - Create a task for creating the "Oxidicom Custom Metadata" `OxidicomAttemptedPushCount=N` file.
/// - Create a [PendingRegistration::End]
fn finish_association(
    ulid: Ulid,
    series_counts: HashMap<SeriesKeySet, usize>,
    files_root: &Arc<Utf8PathBuf>,
) -> Vec<(SeriesKeySet, PendingRegistration)> {
    let mut messages = Vec::with_capacity(series_counts.len() * 2);
    for (series, count) in &series_counts {
        let files_root = Arc::clone(files_root);
        let pacs_file = series.clone().into_oxidicom_custom_pacsfile(
            ulid,
            "OxidicomAttemptedPushCount",
            count.to_string(),
        );
        let task =
            tokio::task::spawn(
                async move { create_blank_file_tokio(&files_root, pacs_file).await },
            );
        messages.push((series.clone(), PendingRegistration::Task(task)));
    }
    let endings = series_counts
        .into_iter()
        .map(|(series, _count)| (series, PendingRegistration::End));
    messages.extend(endings);
    messages
}

/// Create a blank file in place of the [PacsFileRegistrationRequest], and return it if successful.
///
/// Intended to be used for creating "Oxidicom Custom Metadata" files.
fn create_blank_file(
    files_root: &Utf8Path,
    pacs_file: PacsFileRegistrationRequest,
) -> Result<PacsFileRegistrationRequest, ()> {
    let path = files_root.join(&pacs_file.path);
    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent).map_err(|e| {
            tracing::error!(path = parent.as_str(), message = e.to_string());
        })?
    }
    fs_err::File::create(&path).map(|_| pacs_file).map_err(|e| {
        tracing::error!(path = path.into_string(), message = e.to_string());
    })
}

/// Async version of [create_blank_file].
async fn create_blank_file_tokio(
    files_root: &Utf8Path,
    pacs_file: PacsFileRegistrationRequest,
) -> Result<PacsFileRegistrationRequest, ()> {
    let path = files_root.join(&pacs_file.path);
    if let Some(parent) = path.parent() {
        fs_err::tokio::create_dir_all(parent).await.map_err(|e| {
            tracing::error!(path = parent.as_str(), message = e.to_string());
        })?
    }
    fs_err::tokio::File::create(&path)
        .await
        .map(|_| pacs_file)
        .map_err(|e| {
            tracing::error!(path = path.into_string(), message = e.to_string());
        })
}

/// Inserts or updates `association`. If an insert was needed, and also if [Association::pacs_address]
/// is [Some], it will create and return [FindScuParameters].
fn maybe_findscu(
    ulid: Ulid,
    series_key_set: SeriesKeySet,
    association: &mut Association,
) -> Option<FindScuParameters> {
    if let Some(count) = association.series.get_mut(&series_key_set) {
        *count += 1;
        None
    } else {
        let findscu_params = if let Some(pacs_address) = &association.pacs_address {
            let findscu_params = FindScuParameters {
                ulid,
                pacs_address: pacs_address.to_string(),
                aec: association.aec.clone(),
                aet: association.aet.clone(),
                study_instance_uid: series_key_set.StudyInstanceUID.to_string(),
                series_instance_uid: series_key_set.SeriesInstanceUID.to_string(),
            };
            Some(findscu_params)
        } else {
            None
        };
        association.series.insert(series_key_set, 1);
        findscu_params
    }
}

/// Information about a DICOM "association" which is a TCP connection from a PACS server
/// who is pushing DICOM files to us.
struct Association {
    /// AE title of the PACS pushing to us
    aec: ClientAETitle,
    /// Our AE title
    aet: OurAETitle,
    /// Address where we are receiving DICOMs from
    pacs_address: Option<String>,
    /// The unique series we are receiving during this association.
    /// Typically, in _ChRIS_ one series will be pulled per association. However,
    /// it is possible for a PACS server to push any number or fraction of a series to us.
    series: HashMap<SeriesKeySet, usize>,
}

impl Association {
    fn new(aec: ClientAETitle, aet: OurAETitle, pacs_address: Option<String>) -> Self {
        Self {
            aec,
            aet,
            pacs_address,
            series: Default::default(),
        }
    }
}

/// Wraps [write_dicom] with OpenTelemetry logging.
fn store_dicom(files_root: &Utf8Path, pacs_file: &PacsFileRegistration) -> Result<(), ()> {
    match write_dicom(pacs_file, files_root) {
        Ok(path) => tracing::info!(event = "storage", path = path.into_string()),
        Err(e) => {
            tracing::error!(event = "storage", error = e.to_string());
            return Err(());
        }
    }
    Ok(())
}

/// Write a DICOM object to the filesystem.
fn write_dicom<P: AsRef<Utf8Path>>(
    pacs_file: &PacsFileRegistration,
    files_root: P,
) -> Result<Utf8PathBuf, DicomStorageError> {
    let output_path = files_root.as_ref().join(&pacs_file.request.path);
    if let Some(parent_dir) = output_path.parent() {
        fs_err::create_dir_all(parent_dir)?;
    }
    pacs_file.obj.write_to_file(&output_path)?;
    Ok(output_path)
}

/// Report bad tags via OpenTelemetry.
fn report_bad_tags<T: AsRef<[BadTag]>>(
    pacs_file: &PacsFileRegistrationRequest,
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
        path = &pacs_file.path,
        bad_tags = bad_tags_csv
    )
}
