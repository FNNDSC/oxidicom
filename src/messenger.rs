use crate::celery_publisher::CubeRegistrationParams;
use crate::channel_helpers::{send_error_left, send_error_right};
use crate::enums::SeriesEvent;
use crate::error::DicomStorageError;
use crate::lonk::Lonk;
use crate::lonk_publisher::PublishLonkParams;
use crate::types::{DicomInfo, SeriesKey, SeriesPath};
use either::Either;
use std::collections::HashMap;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

type SeriesCounts = HashMap<SeriesKey, u32>;

/// Handles DICOM storage events by producing the appropriate messages for them.
pub(crate) async fn messenger(
    mut receiver: UnboundedReceiver<(
        SeriesKey,
        SeriesEvent<Result<(), DicomStorageError>, DicomInfo<SeriesPath>>,
    )>,
    tx_lonk: &UnboundedSender<PublishLonkParams>,
    tx_celery: &UnboundedSender<CubeRegistrationParams>,
) -> Result<(), SendError<Either<PublishLonkParams, CubeRegistrationParams>>> {
    let mut counts: SeriesCounts = Default::default();
    while let Some((series, event)) = receiver.recv().await {
        let (lonks, rp) = create_messages_for(&mut counts, series, event);
        for lonk in lonks {
            tx_lonk.send(lonk).map_err(send_error_left)?;
        }
        if let Some(registration_params) = rp {
            tx_celery
                .send(registration_params)
                .map_err(send_error_right)?;
        }
    }
    Ok(())
}

/// Produces the messages to handle the given event.
///
/// - On DICOM instance received, publish LONK ndicom message with
///   [crate::lonk_publisher::LonkPriority::Optional] priority level.
/// - On DICOM series finished, publish final LONK ndicom and done
///   messages, and also produce the parameters for CUBE registration.
fn create_messages_for(
    counts: &mut SeriesCounts,
    series_key: SeriesKey,
    event: SeriesEvent<Result<(), DicomStorageError>, DicomInfo<SeriesPath>>,
) -> (Vec<PublishLonkParams>, Option<(DicomInfo<SeriesPath>, u32)>) {
    match event {
        SeriesEvent::Instance(result) => {
            let lonk = count_series(series_key.clone(), counts, result);
            (vec![lonk], None)
        }
        SeriesEvent::Finish(series_info) => {
            let ndicom = counts.remove(&series_key).unwrap_or(0);
            let lonks = vec![
                PublishLonkParams::required(Lonk::ndicom(series_key.clone(), ndicom)),
                PublishLonkParams::last(Lonk::done(series_key)),
            ];
            (lonks, Some((series_info, ndicom)))
        }
    }
}

/// If `result` is success: increment the count for the series.
/// Returns a message which _oxidicom_ should send to NATS conveying the status of `result`.
fn count_series(
    series: SeriesKey,
    counts: &mut SeriesCounts,
    result: Result<(), DicomStorageError>,
) -> PublishLonkParams {
    match result {
        Ok(_) => {
            if let Some(count) = counts.get_mut(&series) {
                *count += 1;
                PublishLonkParams::optional(Lonk::ndicom(series, *count))
            } else {
                counts.insert(series.clone(), 1);
                PublishLonkParams::required(Lonk::ndicom(series, 1))
            }
        }
        Err(e) => PublishLonkParams::required(Lonk::error(series, e)),
    }
}

#[cfg(test)]
mod tests {

    use crate::{lonk_publisher::LonkPriority, AETitle};
    use ulid::Ulid;

    use super::*;
    use crate::lonk::LonkMessage;
    use rstest::*;

    #[rstest]
    fn test_first_instance(series_key: SeriesKey) {
        let mut counts = HashMap::new();
        let event = SeriesEvent::Instance(Ok(()));
        let (lonks, cparams) = create_messages_for(&mut counts, series_key.clone(), event);
        assert!(cparams.is_none());
        assert_eq!(lonks.len(), 1);
        assert_eq!(lonks[0].priority, LonkPriority::Required);
        if let LonkMessage::Ndicom(ndicom) = lonks[0].lonk.message {
            assert_eq!(ndicom, 1);
        } else {
            panic!("First LONK is not a ndicom message.");
        }
        assert_eq!(counts.get(&series_key).copied(), Some(1));
    }

    #[rstest]
    fn test_last_instance(series_key: SeriesKey, dicom_info: DicomInfo<SeriesPath>) {
        let mut counts = [(series_key.clone(), 42)].into();
        let event = SeriesEvent::Finish(dicom_info);
        let (lonks, cparams) = create_messages_for(&mut counts, series_key.clone(), event);
        let (_series_info, ndicom) = cparams.expect("must be Some when series is finished");
        assert_eq!(ndicom, 42);
        assert!(
            counts.get(&series_key).is_none(),
            "Entry should be removed because series is finished"
        );
        assert_eq!(lonks.len(), 2);
        assert_eq!(
            lonks.iter().map(|x| x.priority).collect::<Vec<_>>(),
            vec![LonkPriority::Required, LonkPriority::Last]
        );
        if let LonkMessage::Ndicom(ndicom) = lonks[0].lonk.message {
            assert_eq!(ndicom, 42);
        } else {
            panic!("Second to last LONK is not a ndicom message.");
        }
        assert!(matches!(lonks[1].lonk.message, LonkMessage::Done));
    }

    #[rstest]
    fn test_middle_instance(series_key: SeriesKey) {
        let mut counts = [(series_key.clone(), 41)].into();
        let event = SeriesEvent::Instance(Ok(()));
        let (lonks, cparams) = create_messages_for(&mut counts, series_key.clone(), event);
        assert!(cparams.is_none());
        assert_eq!(lonks.len(), 1);
        assert_eq!(lonks[0].priority, LonkPriority::Optional);
        if let LonkMessage::Ndicom(ndicom) = lonks[0].lonk.message {
            assert_eq!(ndicom, 42);
        } else {
            panic!("Progress message LONK is not a ndicom message.");
        }
        assert_eq!(counts.get(&series_key).copied(), Some(42));
    }

    #[rstest]
    fn test_error(series_key: SeriesKey) {
        let mut counts = HashMap::new();
        let event = SeriesEvent::Instance(Err(DicomStorageError::IO(std::io::Error::new(
            std::io::ErrorKind::Other,
            "pretend error",
        ))));
        let (lonks, cparams) = create_messages_for(&mut counts, series_key, event);
        assert!(
            cparams.is_none(),
            "Should not create message for CUBE DICOM series registration on error"
        );
        assert_eq!(lonks.len(), 1);
        assert_eq!(
            lonks[0].priority,
            LonkPriority::Required,
            "error messages should always have required priority"
        );
        assert!(matches!(lonks[0].lonk.message, LonkMessage::Error(_)));
    }

    #[fixture]
    fn series_key() -> SeriesKey {
        SeriesKey {
            SeriesInstanceUID: "1.2.826.0.1.3680043.8.498.21847029020195636742803265118738348008"
                .to_string(),
            pacs_name: AETitle::from_static("MESNGRTEST"),
            association: Ulid(2109557543540967732464958966464893730),
        }
    }

    #[fixture]
    fn dicom_info(series_key: SeriesKey) -> DicomInfo<SeriesPath> {
        DicomInfo {
            PatientID: "12345678".to_string(),
            StudyDate: time::Date::from_calendar_date(2020, time::Month::April, 18).unwrap(),
            StudyInstanceUID: "1.2.826.0.1.3680043.8.498.37609968233558944170884637276003126876"
                .to_string(),
            SeriesInstanceUID: series_key.SeriesInstanceUID,
            pacs_name: series_key.pacs_name,
            path: SeriesPath::from_static("DUMMY/PATH/FOR/UNIT/TEST/0000.dcm"),
            PatientName: Some("Alice Bar".to_string()),
            PatientBirthDate: Some("19900202".to_string()),
            PatientAge: Some(11033),
            PatientSex: Some("F".to_string()),
            AccessionNumber: Some("123ABC".to_string()),
            Modality: Some("MR".to_string()),
            ProtocolName: Some("Brain Scan".to_string()),
            StudyDescription: Some("I love brains".to_string()),
            SeriesDescription: Some("An example brain scan for software testing".to_string()),
        }
    }
}
