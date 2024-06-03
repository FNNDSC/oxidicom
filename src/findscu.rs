//! DICOM FIND to get NumberOfSeriesRelatedInstances.
//!
//! Mostly based on
//! https://github.com/Enet4/dicom-rs/tree/7c0e5ab895e2f57c432cece41077f13abd4d7f71/findscu

use crate::dicomrs_settings::{ClientAETitle, OurAETitle};
use anyhow::{bail, Context};
use dicom::core::{DataElement, PrimitiveValue, VR};
use dicom::dicom_value;
use dicom::dictionary_std::{tags, uids};
use dicom::encoding::TransferSyntaxIndex;
use dicom::object::{InMemDicomObject, StandardDataDictionary};
use dicom::transfer_syntax::{entries, TransferSyntaxRegistry};
use dicom::ul::pdu::{PDataValue, PDataValueType};
use dicom::ul::{ClientAssociationOptions, Pdu};
use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, KeyValue};
use std::borrow::Cow;
use std::io::Read;
use ulid::Ulid;

pub(crate) struct FindScuParameters {
    pub(crate) ulid: Ulid,
    pub(crate) pacs_address: String,
    pub(crate) aec: ClientAETitle,
    pub(crate) aet: OurAETitle,
    pub(crate) study_instance_uid: String,
    pub(crate) series_instance_uid: String,
}

impl FindScuParameters {
    pub(crate) fn get_number_of_series_related_instances(&self) -> Result<usize, ()> {
        let tracer = global::tracer(env!("CARGO_PKG_NAME"));
        tracer.in_span("findscu", |cx| {
            cx.span().set_attributes(self.to_otel_attributes());
            match self.try_get_number_of_series_related_instances(&cx) {
                Ok(num) => {
                    cx.span().set_status(Status::Ok);
                    Ok(num)
                }
                Err(err) => {
                    tracing::error!(
                        association_ulid = self.ulid.to_string(),
                        pacs_address = &self.pacs_address,
                        aec = self.aec.as_str(),
                        aet = self.aet.as_str(),
                        StudyInstanceUID = &self.study_instance_uid,
                        SeriesInstanceUID = &self.series_instance_uid,
                        message = err.to_string(),
                    );
                    cx.span().set_status(Status::Error {
                        description: Cow::Owned(err.to_string()),
                    });
                    Err(())
                }
            }
        })
    }

    fn try_get_number_of_series_related_instances(
        &self,
        cx: &opentelemetry::Context,
    ) -> anyhow::Result<usize> {
        let abstract_syntax = uids::STUDY_ROOT_QUERY_RETRIEVE_INFORMATION_MODEL_FIND;
        let scu_opt = ClientAssociationOptions::new()
            .with_abstract_syntax(abstract_syntax)
            .calling_ae_title(self.aet.as_str())
            .called_ae_title(self.aec.as_str())
            .max_pdu_length(16384);
        let mut scu = scu_opt.establish_with(&self.pacs_address)?;
        let pc_selected = scu
            .presentation_contexts()
            .first()
            .context("Could not select presentation context")?;
        let pc_selected_id = pc_selected.id;
        let ts = TransferSyntaxRegistry
            .get(&pc_selected.transfer_syntax)
            .context("Poorly negotiated transfer syntax")?;
        let cmd = find_req_command(abstract_syntax, 1);
        let mut cmd_data = Vec::with_capacity(128);
        cmd.write_dataset_with_ts(&mut cmd_data, &entries::IMPLICIT_VR_LITTLE_ENDIAN.erased())
            .context("Failed to write command")?;
        let mut iod_data = Vec::with_capacity(128);
        self.to_dicom_query()
            .write_dataset_with_ts(&mut iod_data, ts)
            .context("failed to write identifier dataset")?;
        let pdu = Pdu::PData {
            data: vec![PDataValue {
                presentation_context_id: pc_selected_id,
                value_type: PDataValueType::Command,
                is_last: true,
                data: cmd_data,
            }],
        };
        scu.send(&pdu).context("Could not send command")?;
        let pdu = Pdu::PData {
            data: vec![PDataValue {
                presentation_context_id: pc_selected_id,
                value_type: PDataValueType::Data,
                is_last: true,
                data: iod_data,
            }],
        };
        scu.send(&pdu).context("Could not send C-Find request")?;

        let mut i = 0;
        let mut dicoms: Vec<InMemDicomObject> = Default::default();
        loop {
            let rsp_pdu = scu
                .receive()
                .context("Failed to receive response from remote node")?;

            match rsp_pdu {
                Pdu::PData { data } => {
                    let data_value = &data[0];

                    let cmd_obj = InMemDicomObject::read_dataset_with_ts(
                        &data_value.data[..],
                        &entries::IMPLICIT_VR_LITTLE_ENDIAN.erased(),
                    )?;
                    let status = cmd_obj
                        .get(tags::STATUS)
                        .context("status code from response is missing")?
                        .to_int::<i64>()
                        .context("failed to read status code")?;
                    if status == 0 {
                        if i == 0 {
                            cx.span().add_event(
                                "status",
                                vec![
                                    KeyValue::new("status", status),
                                    KeyValue::new("description", "No results matching query"),
                                ],
                            )
                        }
                        break;
                    } else if status == 0xFF00 || status == 0xFF01 {
                        // fetch DICOM data
                        let dcm = {
                            let mut rsp = scu.receive_pdata();
                            let mut response_data = Vec::new();
                            rsp.read_to_end(&mut response_data)
                                .context("Failed to read response data")?;

                            InMemDicomObject::read_dataset_with_ts(&response_data[..], ts)
                                .context("Could not read response data set")?
                        };

                        // might be wrong, see https://github.com/Enet4/dicom-rs/issues/479
                        // check DICOM status,
                        // as some implementations might report status code 0
                        // upon sending the response data
                        let status = dcm
                            .get(tags::STATUS)
                            .map(|ele| ele.to_int::<u16>())
                            .transpose()
                            .context("failed to read status code")?
                            .unwrap_or(0);

                        dicoms.push(dcm);

                        if status == 0 {
                            break;
                        }
                        i += 1;
                    } else {
                        cx.span().add_event(
                            "status",
                            vec![
                                KeyValue::new("status", status),
                                KeyValue::new("description", "Operation failed"),
                            ],
                        );
                        break;
                    }
                }

                pdu @ Pdu::Unknown { .. }
                | pdu @ Pdu::AssociationRQ { .. }
                | pdu @ Pdu::AssociationAC { .. }
                | pdu @ Pdu::AssociationRJ { .. }
                | pdu @ Pdu::ReleaseRQ
                | pdu @ Pdu::ReleaseRP
                | pdu @ Pdu::AbortRQ { .. } => {
                    let _ = scu.abort();
                    tracing::error!("Unexpected SCP response: {:?}", pdu);
                    bail!("Unexpected SCP response")
                }
            }
        }
        let _ = scu.release();
        self.get_number_from(dicoms)
    }

    fn to_dicom_query(&self) -> InMemDicomObject {
        let mut obj = InMemDicomObject::new_empty();
        obj.put(DataElement::new(
            tags::QUERY_RETRIEVE_LEVEL,
            VR::CS,
            PrimitiveValue::from("SERIES"),
        ));
        obj.put(DataElement::new(
            tags::STUDY_INSTANCE_UID,
            VR::UI,
            PrimitiveValue::from(self.study_instance_uid.as_str()),
        ));
        obj.put(DataElement::new(
            tags::SERIES_INSTANCE_UID,
            VR::UI,
            PrimitiveValue::from(self.series_instance_uid.as_str()),
        ));
        obj.put(DataElement::new(
            tags::NUMBER_OF_SERIES_RELATED_INSTANCES,
            VR::IS,
            PrimitiveValue::Empty,
        ));
        obj
    }

    /// Extract and parse the value for `NumberOfSeriesRelatedInstances` among several DICOM objects.
    fn get_number_from(
        &self,
        dicoms: impl IntoIterator<Item = InMemDicomObject>,
    ) -> anyhow::Result<usize> {
        dicoms
            .into_iter()
            .filter(|dcm| {
                dcm.get(tags::SERIES_INSTANCE_UID)
                    .and_then(|ele| ele.string().ok())
                    .map(|uid| uid.replace('\0', ""))
                    .is_some_and(|uid| uid.trim() == &self.series_instance_uid)
            })
            .find_map(|dcm| {
                dcm.get(tags::NUMBER_OF_SERIES_RELATED_INSTANCES)
                    .and_then(|ele| ele.string().ok())
                    .and_then(|s| {
                        s.trim()
                            .parse()
                            .map_err(|e| {
                                tracing::warn!(
                                    error = "Invalid number returned from PACS",
                                    pacs_address = &self.pacs_address,
                                    ae_title = self.aec.as_str(),
                                    SeriesInstanceUID = &self.series_instance_uid,
                                    tag = "NumberOfSeriesRelatedInstances",
                                    value = s
                                );
                                e
                            })
                            .ok()
                    })
            })
            .ok_or_else(|| {
                anyhow::Error::msg(
                    "No valid value for NumberOfSeriesRelatedInstances found for the series",
                )
            })
    }

    fn to_otel_attributes(&self) -> Vec<KeyValue> {
        vec![
            KeyValue::new("association_ulid", self.ulid.to_string()),
            KeyValue::new("pacs_address", self.pacs_address.to_string()),
            KeyValue::new("aec", self.aec.to_string()),
            KeyValue::new("aet", self.aet.to_string()),
            KeyValue::new("StudyInstanceUID", self.study_instance_uid.to_string()),
            KeyValue::new("SeriesInstanceUID", self.series_instance_uid.to_string()),
        ]
    }
}

fn find_req_command(
    sop_class_uid: &str,
    message_id: u16,
) -> InMemDicomObject<StandardDataDictionary> {
    InMemDicomObject::command_from_element_iter([
        // SOP Class UID
        DataElement::new(
            tags::AFFECTED_SOP_CLASS_UID,
            VR::UI,
            PrimitiveValue::from(sop_class_uid),
        ),
        // command field
        DataElement::new(
            tags::COMMAND_FIELD,
            VR::US,
            // 0020H: C-FIND-RQ message
            dicom_value!(U16, [0x0020]),
        ),
        // message ID
        DataElement::new(tags::MESSAGE_ID, VR::US, dicom_value!(U16, [message_id])),
        //priority
        DataElement::new(
            tags::PRIORITY,
            VR::US,
            // medium
            dicom_value!(U16, [0x0000]),
        ),
        // data set type
        DataElement::new(
            tags::COMMAND_DATA_SET_TYPE,
            VR::US,
            dicom_value!(U16, [0x0001]),
        ),
    ])
}
