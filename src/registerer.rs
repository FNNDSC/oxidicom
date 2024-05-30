use std::sync::Arc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::batcher::Batcher;
use crate::chrisdb_client::{CubePostgresClient, PacsFileDatabaseError};
use crate::error::HandleLoopError;
use crate::pacs_file::PacsFileRegistrationRequest;

/// Forward objects from `receiver` to the given `client`.
///
/// - Received `Some`: add item to the batch. When batch is full, give everything to the `client`
/// - Received `None`: flush current batch to the `client`
pub async fn cube_pacsfile_registerer(
    mut receiver: UnboundedReceiver<Option<PacsFileRegistrationRequest>>,
    client: CubePostgresClient,
    batch_size: usize,
) -> Result<(), HandleLoopError> {
    // We have two loops:
    // 1. The receiver loop receives DICOM metadata from the receiver, and adds them to a batch.
    //    When the batch is full, we create a task to send the DICOM metadata to the database.
    // 2. The joiner_loop simply blocks until every task is complete.
    let client = Arc::new(client);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let receiver_loop = async {
        let mut batches = Batcher::new(batch_size);
        while let Some(event) = receiver.recv().await {
            batches = handle_event(event, batches, &client, &tx).unwrap();
        }
        drop(tx);
        flush_to_database(batches, client).await
    };

    // join tasks and take note of any errors.
    let mut everything_ok = true;
    let joiner_loop = async {
        while let Some(task) = rx.recv().await {
            if task.await.unwrap().is_err() {
                everything_ok = false;
            }
        }
    };

    let (last_flush, _) = tokio::join!(receiver_loop, joiner_loop);
    last_flush
        .map_err(|_| HandleLoopError("Last flush of PACS files metadata to database failed."))?;
    if everything_ok {
        Ok(())
    } else {
        Err(HandleLoopError(
            "There was an error registering PACS files metadata to the database.",
        ))
    }
}

/// A tokio task of [CubePostgresClient::register]
type RegistrationTask = JoinHandle<Result<(), PacsFileDatabaseError>>;

/// Receives `event` and calls [register_task] when needed, sending the task to `tx`.
///
/// Returns the batch's next state.
fn handle_event(
    event: Option<PacsFileRegistrationRequest>,
    prev: Batcher<PacsFileRegistrationRequest>,
    client: &Arc<CubePostgresClient>,
    tx: &UnboundedSender<RegistrationTask>,
) -> Result<Batcher<PacsFileRegistrationRequest>, SendError<RegistrationTask>> {
    let (next, full_batch) = match event {
        None => take_batch(prev),
        Some(pacs_file) => prev.push(pacs_file),
    };
    if let Some(files) = full_batch {
        let task = register_task(client, files);
        tx.send(task)?;
    }
    Ok(next)
}

/// Empties the batch and returns its contents.
fn take_batch<T>(batches: Batcher<T>) -> (Batcher<T>, Option<Vec<T>>) {
    let batch_size = batches.batch_size;
    let batch = batches.into_inner();
    let next_batches = Batcher::new(batch_size);
    if batch.is_empty() {
        tracing::warn!("batch is empty");
        (next_batches, None)
    } else {
        (next_batches, Some(batch))
    }
}

/// Wraps [CubePostgresClient::register] with [tokio::spawn] and [tracing].
fn register_task(
    client: &Arc<CubePostgresClient>,
    files: Vec<PacsFileRegistrationRequest>,
) -> RegistrationTask {
    let client = Arc::clone(client);
    tokio::spawn(async move {
        let n_files = files.len();
        let result = client.register(files).await;
        match &result {
            Ok(_) => {
                tracing::info!(task = "register", count = n_files);
            }
            Err(e) => {
                tracing::error!(task = "register", error = e.to_string());
            }
        }
        result
    })
}

/// Consume the `batch` and give everything to [CubePostgresClient::register]
async fn flush_to_database<C: AsRef<CubePostgresClient>>(
    batch: Batcher<PacsFileRegistrationRequest>,
    client: C,
) -> Result<(), PacsFileDatabaseError> {
    let remaining = batch.into_inner();
    if remaining.is_empty() {
        Ok(())
    } else {
        client.as_ref().register(&remaining).await
    }
}
