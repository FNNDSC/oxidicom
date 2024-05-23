use tokio::sync::mpsc::UnboundedReceiver;
use crate::event::AssociationEvent;

/// Write received DICOMs to storage.
pub async fn dicom_storage_writer(mut receiver: UnboundedReceiver<AssociationEvent>) -> anyhow::Result<()> {
    while let Some(event) = receiver.recv().await {
        match event {
            AssociationEvent::Start { .. } => { tracing::info!("Association started") }
            AssociationEvent::DicomInstance { .. } => { tracing::info!("got DICOM") }
            AssociationEvent::Finish { .. } => { tracing::info!("Association finished") }
        }
    }
    Ok(())
}