use crate::cube_client::{CubePacsStorageClient, PacsFileRegistration};
use crate::custom_metadata::SeriesKeySet;
use crate::dicomrs_options::{ClientAETitle, OurAETitle};
use crate::error::ChrisPacsError;
use crate::event::AssociationEvent;
use crate::pacs_file::{tt, PacsFileRegistrationRequest, PacsFileResponse};
use dicom::dictionary_std::tags;
use dicom::object::DefaultDicomObject;
use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, KeyValue};
use std::borrow::Cow;
use std::collections::HashMap;
use std::net::SocketAddrV4;
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
        if let Some(count) = association.pushed.get_mut(&series_key) {
            *count += 1;
            tracing::info!(
                association_uuid = uuid.hyphenated().to_string(),
                stage = "middle",
                SeriesInstanceUID = &series_key.SeriesInstanceUID,
                pushed = count,
                received = association.received
            );
        } else {
            tracing::info!(
                association_uuid = uuid.hyphenated().to_string(),
                stage = "first",
                SeriesInstanceUID = &series_key.SeriesInstanceUID
            );
            association.pushed.insert(series_key, 1);
        };
        let total_pushed: usize = association.pushed.values().sum();
        assert!(total_pushed <= association.received);
        if association.finished && association.received == total_pushed {
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
    pacs_address: Option<SocketAddrV4>,
    /// Number of DICOM files received, which may either be pending or pushed
    received: usize,
    /// Number of DICOM files which were pushed to CUBE (or attempted to push,
    /// but had an error and gave up).
    pushed: HashMap<SeriesKeySet, usize>,
    /// Will there be more DICOM files?
    finished: bool,
}

impl Association {
    fn new(aec: ClientAETitle, aet: OurAETitle, peer_address: Option<SocketAddrV4>) -> Self {
        Self {
            aec,
            aet,
            pacs_address: peer_address,
            pushed: Default::default(),
            received: 0,
            finished: false,
        }
    }

    fn find_request(&self, uuid: Uuid, dcm: &DefaultDicomObject) -> Option<FindScuParameters> {
        if self.received != 0 {
            return None;
        }
        // note: findscu parameters are intentionally unsanitized
        let study_instance_uid = dcm
            .element(tags::STUDY_INSTANCE_UID)
            .ok()
            .and_then(|e| e.string().ok())?
            .to_string();
        self.pacs_address.map(|pacs_address| FindScuParameters {
            uuid,
            pacs_address,
            aec: self.aec.clone(),
            aet: self.aet.clone(),
            study_instance_uid,
            series_instance_uid: dcm
                .element(tags::SERIES_INSTANCE_UID)
                .ok()
                .and_then(|e| e.string().ok())
                .map(|s| s.to_string()),
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
        pacs_address: Option<SocketAddrV4>,
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
            let total_pushed: usize = association.pushed.values().sum();
            if total_pushed == association.received {
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

    fn jobs_for_instance(&self, uuid: Uuid, dcm: DefaultDicomObject) -> Vec<ChrisSenderJobAction> {
        let mut associations = self.0.associations.lock().unwrap();
        let association = associations.get_mut(&uuid).expect("Unknown UUID");
        association.received += 1;
        let find_job = association
            .find_request(uuid, &dcm)
            .map(|params| ChrisSenderJobAction::GetNumberOfRelatedInstances { uuid, params });
        let push_job = match PacsFileRegistration::new(association.aec.clone(), dcm) {
            Ok((pacs_file, bad_tags)) => {
                if !bad_tags.is_empty() {
                    // let bts: Vec<_> = bad_tags
                    //     .into_iter()
                    //     .map(|b| b.to_string())
                    //     .map(StringValue::from)
                    //     .collect();
                    // let value = Value::Array(Array::String(bts));
                    // cx.span().set_attribute(KeyValue::new("bad_tags", value))
                    tracing::warn!(
                        path = &pacs_file.request.path,
                        bad_tags = bad_tags
                            .into_iter()
                            .map(|b| b.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                }
                ChrisSenderJobAction::PushDicom { uuid, pacs_file }
            }
            Err((e, obj)) => {
                tracing::error!(
                    missing_required_tag = e.to_string(),
                    StudyInstanceUID = tt(&obj, tags::STUDY_INSTANCE_UID),
                    SeriesInstanceUID = tt(&obj, tags::SERIES_INSTANCE_UID),
                    SOPInstanceUID = tt(&obj, tags::SOP_INSTANCE_UID),
                    InstanceNumber = tt(&obj, tags::INSTANCE_NUMBER)
                );
                return Vec::with_capacity(0);
                // TODO push error information to CUBE
            }
        };
        let jobs = if let Some(find_job) = find_job {
            vec![find_job, push_job]
        } else {
            vec![push_job]
        };
        jobs
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

/// A job which performs I/O to handle an [AssociationEvent].
pub(crate) struct ChrisSenderJob {
    sender: Arc<ChrisSenderInternal>,
    action: ChrisSenderJobAction,
}

enum ChrisSenderJobAction {
    /// Send a C-FIND message to the PACS server to get `NumberOfSeriesRelatedInstances`.
    GetNumberOfRelatedInstances {
        uuid: Uuid,
        params: FindScuParameters,
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

// TODO
// /usr/bin/findscu -xi -S  -k AccessionNumber -k AcquisitionProtocolDescription
// -k AcquisitionProtocolName -k InstanceNumber -k ModalitiesInStudy -k Modality
// -k NumberOfPatientRelatedInstances -k NumberOfPatientRelatedSeries
// -k NumberOfPatientRelatedStudies -k NumberOfSeriesRelatedInstances
// -k NumberOfStudyRelatedInstances -k NumberOfStudyRelatedSeries -k PatientAge
// -k PatientBirthDate -k PatientID -k PatientName -k PatientSex -k PerformedStationAETitle
// -k ProtocolName -k "QueryRetrieveLevel=STUDY" -k SeriesDate -k SeriesDescription
// -k SeriesInstanceUID -k StudyDate -k StudyDescription
// -k "StudyInstanceUID=x.x.x.xxxxx"  -aec ORTHANC -aet CHRISLOCAL
pub(crate) struct FindScuParameters {
    uuid: Uuid,
    pacs_address: SocketAddrV4,
    aec: ClientAETitle,
    aet: OurAETitle,
    study_instance_uid: String,
    series_instance_uid: Option<String>,
}

impl ChrisSenderJob {
    /// Run this job. Since the job will do I/O, it is recommended to move this to a thread.
    pub(crate) fn run(self) -> Result<(), ChrisPacsError> {
        match self.action {
            ChrisSenderJobAction::GetNumberOfRelatedInstances { uuid, params } => {
                getset_number_of_dicom_instances(&self.sender, uuid, params)
            }
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

/// Send a C-FIND request to the PACS server to obtain `NumberOfSeriesRelatedInstances`.
/// Store this information in _CUBE_ via the "Oxidicom Custom Metadata" spec.
fn getset_number_of_dicom_instances(
    client: &ChrisSenderInternal,
    uuid: Uuid,
    params: FindScuParameters,
) -> Result<(), ChrisPacsError> {
    tracing::warn!("getset_number_of_dicom_instances not implemented");
    Ok(())
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
