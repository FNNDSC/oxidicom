use camino::{Utf8Path, Utf8PathBuf};
use dicom::object::DefaultDicomObject;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::dicomrs_options::ClientAETitle;
use crate::error::{DicomStorageError, HandleLoopError};
use crate::event::AssociationEvent;
use crate::pacs_file::{PacsFileRegistration, PacsFileRegistrationRequest};

/// Write incoming DICOMs from `receiver` to storage. DICOM metadata of successfully stored
/// DICOM files are sent to `registration_sender`.
pub async fn dicom_storage_writer(
    mut receiver: UnboundedReceiver<AssociationEvent>,
    registration_sender: UnboundedSender<Option<PacsFileRegistrationRequest>>,
    files_root: Utf8PathBuf,
) -> Result<(), HandleLoopError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // We have two loops:
    // 1. The dispatch loop receives DICOM objects, and uses tokio::spawn to store the incoming DICOM
    // 2. The results are sent from dispatch_loop to join_loop, which:
    //    - forwards DICOM metadata to `registration_sender` (to be registered to CUBE database)
    //    - joins completed tasks from dispatch_loop
    //    - blocks dicom_storage_writer until all tasks created here by tokio::spawn are joined
    //    - collects errors which happened from the tasks
    let dispatch_loop = async {
        let files_root = Arc::new(files_root);
        while let Some(event) = receiver.recv().await {
            match event {
                // Do nothing at the start of an association.
                AssociationEvent::Start { .. } => {}
                // Store received DICOMs.
                AssociationEvent::DicomInstance { aec, dcm, .. } => {
                    let files_root = Arc::clone(&files_root);
                    let handle = tokio::task::spawn_blocking(move || {
                        store_dicom(files_root.as_path(), aec, dcm)
                    });
                    tx.send(handle).unwrap()
                }
                // When an association is finished, we send `None` which tells the
                // receiving code to flush items to the database.
                AssociationEvent::Finish { .. } => registration_sender.send(None).unwrap(),
            }
        }
        drop(tx);
    };
    let mut everything_ok = true;
    let join_loop = async {
        while let Some(handle) = rx.recv().await {
            if let Ok(dcm) = handle.await.unwrap() {
                // on successful DICOM file storage, send the returned DICOM metadata
                // for registration to CUBE's database
                registration_sender.send(Some(dcm.request)).unwrap()
            } else {
                everything_ok = false;
            }
        }
        everything_ok
    };
    let (_, everything_ok) = tokio::join!(dispatch_loop, join_loop);
    if everything_ok {
        Ok(())
    } else {
        Err(HandleLoopError(
            "There was an error writing DICOMs to storage.",
        ))
    }
}

/// Wraps [write_dicom] with OpenTelemetry logging.
fn store_dicom<P: AsRef<Utf8Path>>(
    files_root: P,
    pacs_name: ClientAETitle,
    obj: DefaultDicomObject,
) -> Result<PacsFileRegistration, DicomStorageError> {
    let (dcm, bad_tags) = PacsFileRegistration::new(pacs_name, obj)?;
    match write_dicom(&dcm, files_root) {
        Ok(path) => tracing::info!(event = "storage", path = path.into_string()),
        Err(e) => {
            tracing::error!(event = "storage", error = e.to_string());
            return Err(e);
        }
    }
    Ok(dcm)
}

/// Write a DICOM object to the filesystem.
fn write_dicom<P: AsRef<Utf8Path>>(
    dcm: &PacsFileRegistration,
    files_root: P,
) -> Result<Utf8PathBuf, DicomStorageError> {
    let output_path = files_root.as_ref().join(&dcm.request.path);
    if let Some(parent_dir) = output_path.parent() {
        fs_err::create_dir_all(parent_dir)?;
    }
    dcm.obj.write_to_file(&output_path)?;
    Ok(output_path)
}
