use crate::cube_client::{CubePacsStorageClient, PacsFileRegistration};
use crate::custom_metadata::getset_number_of_dicom_instances;
use crate::dicomrs_options::{ClientAETitle, OurAETitle};
use crate::error::ChrisPacsError;
use crate::event::AssociationEvent;
use crate::findscu::FindScuParameters;
use crate::pacs_file::{tt, BadTag, PacsFileRegistrationRequest, PacsFileResponse};
use crate::series_key_set::SeriesKeySet;
use dicom::dictionary_std::tags;
use dicom::object::DefaultDicomObject;
use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, KeyValue};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Wrapper around [CubePacsStorageClient] which is aware of which association
/// DICOM files come from. The extra information is used to provide:
///
/// - Traces to OpenTelemetry Collector
/// - Metadata about the DICOM series to CUBE via the "Oxidicom Custom Metadata" spec.
pub(crate) struct ChrisSender(Arc<ChrisSenderInternal>);

struct ChrisSenderInternal {
    client: CubePacsStorageClient,
    associations: Mutex<HashMap<Uuid, Association>>,
}

impl ChrisSenderInternal {
    fn new(client: CubePacsStorageClient) -> Self {
        Self {
            client,
            associations: Mutex::new(Default::default()),
        }
    }

    // would be convenient, but I can't figure out the lifetimes.
    // fn get_mut(&self, uuid: &Uuid) -> (&mut Association, MutexGuard<HashMap<Uuid, Association>>) {
    //     let mut raii = self.associations.lock().unwrap();
    //     let association = raii.get_mut(uuid).expect("Unknown association_uuid");
    //     (association, raii)
    // }

    /// Increment the pushed counter for an association.
    ///
    /// Returns `Some` if this was the last file to push.
    fn increment_pushed(
        &self,
        uuid: Uuid,
        pacs_file: PacsFileRegistrationRequest,
    ) -> Option<Association> {
        let mut associations = self.associations.lock().unwrap();
        let mut association = associations
            .remove(&uuid)
            .expect("Unknown association UUID");
        let series_key: SeriesKeySet = pacs_file.into();
        let series = association
            .series
            .get_mut(&series_key)
            .expect("Unknown series");
        series.pushed += 1;
        assert!(series.pushed <= series.received);
        tracing::info!(
            association_uuid = uuid.hyphenated().to_string(),
            SeriesInstanceUID = &series_key.SeriesInstanceUID,
            pushed = series.pushed,
            received = series.received
        );
        if association.finished && association.series.values().all(SeriesProgress::is_uptodate) {
            Some(association)
        } else {
            associations.insert(uuid, association); // put it back
            None
        }
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
    /// it is possible for a PACS server to push any number of series it wants to.
    series: HashMap<SeriesKeySet, SeriesProgress>,
    /// Will there be more DICOM files?
    finished: bool,
}

#[derive(Debug, Copy, Clone)]
struct SeriesProgress {
    /// Number of DICOM files received, which may either be pending, pushed, or given up.
    received: usize,
    /// Number of DICOM files which were pushed to CUBE or given up.
    pushed: usize,
}

impl Default for SeriesProgress {
    fn default() -> Self {
        // at least 1 instance of a series must have been received in order for us to be aware of it
        Self {
            received: 1,
            pushed: 0,
        }
    }
}

impl SeriesProgress {
    fn is_uptodate(&self) -> bool {
        self.received == self.pushed
    }
}

impl Association {
    fn new(aec: ClientAETitle, aet: OurAETitle, pacs_address: Option<String>) -> Self {
        Self {
            aec,
            aet,
            pacs_address,
            series: Default::default(),
            finished: false,
        }
    }

    /// Return the parameters needed to query the PACS for the `NumberOfSeriesRelatedInstances`
    /// of the given DICOM object. Returns `None` if it is not possible to get the parameters.
    fn find_request(&self, uuid: Uuid, dcm: &DefaultDicomObject) -> Option<FindScuParameters> {
        // note: findscu parameters are intentionally unsanitized
        let study_instance_uid = dcm
            .element(tags::STUDY_INSTANCE_UID)
            .ok()
            .and_then(|e| e.string().ok())?
            .to_string();
        let series_instance_uid = dcm
            .element(tags::SERIES_INSTANCE_UID)
            .ok()
            .and_then(|e| e.string().ok())?
            .to_string();
        self.pacs_address
            .clone()
            .map(|pacs_address| FindScuParameters {
                uuid,
                pacs_address,
                aec: self.aec.clone(),
                aet: self.aet.clone(),
                study_instance_uid,
                series_instance_uid,
            })
    }
}

impl ChrisSender {
    pub(crate) fn new(client: CubePacsStorageClient) -> Self {
        Self(Arc::new(ChrisSenderInternal::new(client)))
    }

    /// Produce I/O jobs for handling the event.
    ///
    /// ## Panics
    ///
    /// Panics in case of any programmer error, e.g.
    ///
    /// - An association is started with a non-unique UUID
    /// - A [AssociationEvent::DicomInstance] was received before a [AssociationEvent::Start]
    ///   was received for the given UUID
    /// - An association is finished twice with the same UUID
    pub(crate) fn prepare_jobs_for(&self, event: AssociationEvent) -> Vec<ChrisSenderJob> {
        match event {
            AssociationEvent::Start {
                uuid,
                aec,
                aet,
                pacs_address,
            } => {
                self.start_association(uuid, aec, aet, pacs_address);
                Vec::with_capacity(0)
            }
            AssociationEvent::DicomInstance { uuid, dcm } => {
                self.with_sender(self.jobs_for_instance(uuid, dcm))
            }
            AssociationEvent::Finish { uuid, ok } => {
                self.with_sender(self.set_association_finished(uuid, ok))
            }
        }
    }

    fn start_association(
        &self,
        uuid: Uuid,
        aec: ClientAETitle,
        aet: OurAETitle,
        pacs_address: Option<String>,
    ) {
        let prev = self
            .0
            .associations
            .lock()
            .unwrap()
            .insert(uuid, Association::new(aec, aet, pacs_address));
        if prev.is_some() {
            panic!("Duplicate association UUID: {uuid}")
        }
    }

    fn set_association_finished(&self, uuid: Uuid, ok: bool) -> Option<ChrisSenderJobAction> {
        let mut associations = self.0.associations.lock().unwrap();
        if let Some(mut association) = associations.remove(&uuid) {
            if association.series.values().all(SeriesProgress::is_uptodate) {
                // everything done
                Some(ChrisSenderJobAction::Finalize { uuid, association })
            } else {
                // not all files have been pushed
                association.finished = true;
                associations.insert(uuid, association);
                None
            }
        } else {
            if ok {
                // should only ever be called with ok=true once
                panic!("Finished association_uuid={} is unknown. Either AssociationEvent::Start was never received, or AssociationEvent::Finish was received twice.", uuid)
            } else {
                // AssociationEvent::Start might not be called if the client violates
                // the DIMSE protocol by never providing its AE title.
                tracing::debug!("Ignoring unknown finished association uuid={}", uuid)
            }
            None
        }
    }

    /// Decide which jobs to run for an incoming DICOM instance.
    ///
    /// - If the DICOM instance has all the required tags, then it needs to be registered to CUBE.
    /// - If the DICOM instance is the first of its series to be received, then we also want to
    ///   contact the PACS server and ask it for the `NumberOfSeriesRelatedInstances`.
    fn jobs_for_instance(&self, uuid: Uuid, dcm: DefaultDicomObject) -> Vec<ChrisSenderJobAction> {
        let mut associations = self.0.associations.lock().unwrap();
        let association = associations.get_mut(&uuid).expect("Unknown UUID");
        let findscu_params = association.find_request(uuid, &dcm);
        match PacsFileRegistration::new(association.aec.clone(), dcm) {
            Ok((pacs_file, bad_tags)) => {
                warn_for_bad_tags(&pacs_file.request.path, &bad_tags);

                let series_key = SeriesKeySet::from(pacs_file.request.clone());
                if let Some(series) = association.series.get_mut(&series_key) {
                    series.received += 1;
                    vec![ChrisSenderJobAction::PushDicom { uuid, pacs_file }]
                } else {
                    // received first instance of a series
                    association
                        .series
                        .insert(series_key.clone(), Default::default());
                    vec![
                        ChrisSenderJobAction::PushDicom { uuid, pacs_file },
                        ChrisSenderJobAction::GetNumberOfRelatedInstances {
                            uuid,
                            series: series_key,
                            params: findscu_params,
                        },
                    ]
                }
            }
            Err((e, obj)) => {
                // TODO push error information to CUBE
                tracing::error!(
                    missing_required_tag = e.to_string(),
                    StudyInstanceUID = tt(&obj, tags::STUDY_INSTANCE_UID),
                    SeriesInstanceUID = tt(&obj, tags::SERIES_INSTANCE_UID),
                    SOPInstanceUID = tt(&obj, tags::SOP_INSTANCE_UID),
                    InstanceNumber = tt(&obj, tags::INSTANCE_NUMBER)
                );
                Vec::with_capacity(0)
            }
        }
    }

    fn with_sender(
        &self,
        actions: impl IntoIterator<Item = ChrisSenderJobAction>,
    ) -> Vec<ChrisSenderJob> {
        actions
            .into_iter()
            .map(|action| ChrisSenderJob {
                sender: Arc::clone(&self.0),
                action,
            })
            .collect()
    }
}

fn warn_for_bad_tags(pacsfile_path: &str, bad_tags: &[BadTag]) {
    if bad_tags.is_empty() {
        return;
    }
    // let bts: Vec<_> = bad_tags
    //     .into_iter()
    //     .map(|b| b.to_string())
    //     .map(StringValue::from)
    //     .collect();
    // let value = Value::Array(Array::String(bts));
    // cx.span().set_attribute(KeyValue::new("bad_tags", value))
    tracing::warn!(
        pacsfile_path = pacsfile_path,
        bad_tags = bad_tags
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
}

/// A job which performs I/O to handle an [AssociationEvent].
pub(crate) struct ChrisSenderJob {
    sender: Arc<ChrisSenderInternal>,
    action: ChrisSenderJobAction,
}

enum ChrisSenderJobAction {
    /// Send a C-FIND message to the PACS server to get `NumberOfSeriesRelatedInstances`.
    GetNumberOfRelatedInstances {
        /// Association UUID
        uuid: Uuid,
        /// Series to get number of instances for
        series: SeriesKeySet,
        /// Parameters for `findscu`. Might be `None` if parameters cannot be determined.
        params: Option<FindScuParameters>,
    },
    PushDicom {
        uuid: Uuid,
        pacs_file: PacsFileRegistration,
    },
    Finalize {
        uuid: Uuid,
        association: Association,
    },
}

impl ChrisSenderJob {
    /// Run this job. Since the job will do I/O, it is recommended to move this to a thread.
    pub(crate) fn run(self) -> Result<(), ChrisPacsError> {
        match self.action {
            ChrisSenderJobAction::GetNumberOfRelatedInstances {
                uuid,
                series,
                params,
            } => getset_number_of_dicom_instances(&self.sender.client, uuid, params, series),
            ChrisSenderJobAction::PushDicom { uuid, pacs_file } => {
                push_dicom_wrapper(&self.sender, uuid, pacs_file)
            }
            ChrisSenderJobAction::Finalize { uuid, association } => {
                register_end_of_association(&self.sender, uuid, association)
                    .into_iter()
                    .find_map(|r| r.err())
                    .map(|e| Err(e))
                    .unwrap_or(Ok(()))
            }
        }
    }
}

/// Push DICOMs to CUBE. This function calls [CubePacsStorageClient::store] with
/// extra information from [ChrisSenderInternal] and wraps everything in an OpenTelemetry span.
fn push_dicom_wrapper(
    sender: &ChrisSenderInternal,
    uuid: Uuid,
    pacs_file: PacsFileRegistration,
) -> Result<(), ChrisPacsError> {
    let tracer = global::tracer(env!("CARGO_PKG_NAME"));
    tracer.in_span("push_to_chris", |cx| {
        let results = push_dicom(sender, uuid, pacs_file);
        if let Some(Ok(pacs_file)) = results.first() {
            let a = [
                KeyValue::new("SeriesInstanceUID", pacs_file.SeriesInstanceUID.to_string()),
                KeyValue::new("fname", pacs_file.fname.to_string()),
                KeyValue::new("url", pacs_file.url.to_string()),
            ];
            cx.span().set_attributes(a)
        }
        let first_error = results.into_iter().find_map(|r| r.err());
        if let Some(e) = first_error {
            cx.span().set_status(Status::Error {
                description: Cow::Owned(e.to_string()),
            });
            Err(e)
        } else {
            cx.span().set_status(Status::Ok);
            Ok(())
        }
    })
}

/// Push a DICOM file to CUBE and increment the counter. If this was the last file to be pushed
/// for the current association, also calls [register_end_of_association].
fn push_dicom(
    sender: &ChrisSenderInternal,
    uuid: Uuid,
    pacs_file: PacsFileRegistration,
) -> Vec<Result<PacsFileResponse, ChrisPacsError>> {
    let mut result = vec![sender.client.store(&pacs_file)];
    if let Some(association) = sender.increment_pushed(uuid, pacs_file.request) {
        result.extend(register_end_of_association(sender, uuid, association))
    }
    result
}

/// Indicate that we are done pushing files, as according to the "Oxidicom Custom Metadata" Spec.
fn register_end_of_association(
    sender: &ChrisSenderInternal,
    uuid: Uuid,
    association: Association,
) -> Vec<Result<PacsFileResponse, ChrisPacsError>> {
    tracing::info!(
        association_uuid = uuid.hyphenated().to_string(),
        stage = "end"
    );
    Vec::with_capacity(0)
}
