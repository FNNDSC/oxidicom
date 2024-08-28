//! Celery task definition of the PACSSeries registration function in CUBE,
//! for submitting tasks to CUBE (Python)'s celery worker from our Rust code.

#![allow(unused_variables)]
#![allow(unreachable_code)]

use std::num::NonZeroUsize;

/// A function stub with the same signature as the `register_pacs_series` celery task
/// in *CUBE*'s Python code.
#[celery::task(name = "pacsfiles.tasks.register_pacs_series")]
fn register_pacs_series(
    patient_id: String,
    patient_name: String,
    study_date: String,
    study_instance_uid: String,
    study_description: String,
    series_description: String,
    series_instance_uid: String,
    pacs_name: String,
    path: String,
    ndicom: NonZeroUsize,
) {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;

    #[tokio::test]
    async fn test_try_celery_wip() -> anyhow::Result<()> {
        let app = celery::app!(
            broker = AMQPBroker { "amqp://queue:5672" },
            tasks = [register_pacs_series],
            task_routes = [ "pacsfiles.tasks.register_pacs_series" => "main2" ],
        )
        .await?;

        let task = register_pacs_series::new(
            "Jennings Zhang".to_string(),
            "abc123ismyID".to_string(),
            "2024-08-28".to_string(),
            "StudyInstance123".to_string(),
            "hello from rust".to_string(),
            "SeriesInstance456".to_string(),
            "i am so cool".to_string(),
            "MyPACS".to_string(),
            "SERVICES/PACS/MyPACS/123456-crazy/brain_crazy_study/SAG_T1_MPRA".to_string(),
            NonZeroUsize::new(192).unwrap(),
        );
        app.send_task(task).await?;
        Ok(())
    }
}
