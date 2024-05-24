use std::path::PathBuf;
use camino::{Utf8Path, Utf8PathBuf};
use dicom::object::DefaultDicomObject;
use futures::TryFutureExt;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::dicomrs_options::ClientAETitle;
use crate::error::DicomStorageError;
use crate::event::AssociationEvent;
use crate::pacs_file::PacsFileRegistration;

/// Write received DICOMs to storage.
pub async fn dicom_storage_writer(
    mut receiver: UnboundedReceiver<AssociationEvent>,
    files_root: Utf8PathBuf,
) -> anyhow::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // We have two loops:
    // 1. The dispatch loop receives DICOM objects, and uses tokio::spawn to handle the incoming DICOM
    // 2. The results are sent from dispatch_loop to join_loop, which:
    //    - joins completed tasks from dispatch_loop
    //    - blocks dicom_storage_writer until all tasks created here by tokio::spawn are joined
    //    - collects errors which happened from the tasks
    let dispatch_loop = async {
        let storage_writer = Arc::new(DicomStorageWriter::new(files_root));
        while let Some(event) = receiver.recv().await {
            let storage_writer = Arc::clone(&storage_writer);
            let handle = tokio::task::spawn_blocking(move || storage_writer.handle_event(event));
            tx.send(handle).unwrap()
        }
    };
    let join_loop = async {
        let mut everything_ok = true;
        while let Some(handle) = rx.recv().await {
            let result = handle.await.unwrap();
            if result.is_err() {
                everything_ok = false;
            }
        }
        everything_ok
    };
    let (_, everything_ok) = tokio::join!(dispatch_loop, join_loop);
    if everything_ok {
        anyhow::Ok(())
    } else {
        Err(anyhow::Error::msg(
            "There was an error writing DICOM files to storage.",
        ))
    }
}

struct DicomStorageWriter {
    files_root: Utf8PathBuf,
}

impl DicomStorageWriter {
    fn new(files_root: Utf8PathBuf) -> Self {
        Self { files_root }
    }

    fn handle_event(&self, event: AssociationEvent) -> Result<(), ()> {
        match event {
            AssociationEvent::Start {
                ulid,
                aec,
                aet,
                pacs_address,
            } => {
                Ok(()) // TODO
            }
            AssociationEvent::DicomInstance { ulid, aec, dcm } => {
                self.on_dicom(aec, dcm).map_err(|_| ())
            }
            AssociationEvent::Finish { ulid, ok } => {
                Ok(()) // TODO
            }
        }
    }

    fn on_dicom(&self, pacs_name: ClientAETitle, obj: DefaultDicomObject) -> Result<(), DicomStorageError> {
        let (dcm, bad_tags) = PacsFileRegistration::new(pacs_name, obj)?;
        // TODO use bad_tags
        match write_dicom(&dcm, &self.files_root) {
            Ok(path) => tracing::info!(event = "storage", path = path.into_string()),
            Err(e) => {
                tracing::error!(event = "storage", error = e.to_string());
                return Err(e);
            }
        }
        // TODO register DICOM to database

        Ok(())
    }
}

/// Write a DICOM object to the filesystem.
fn write_dicom<P: AsRef<Utf8Path>>(
    dcm: &PacsFileRegistration,
    files_root: P,
) -> Result<Utf8PathBuf, DicomStorageError> {
    let output_path = files_root.as_ref().join(&dcm.request.path);
    dcm.obj.write_to_file(&output_path)?;
    Ok(output_path)
}
