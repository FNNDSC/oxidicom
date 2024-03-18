//! Handles incoming request to store a DICOM file.
//!
//! File mostly copied from dicom-rs.
//! https://github.com/Enet4/dicom-rs/blob/dbd41ed3a0d1536747c6b8ea2b286e4c6e8ccc8a/storescp/src/main.rs

use std::net::{SocketAddrV4, TcpStream};
use std::sync::mpsc::Sender;

use dicom::core::{DataElement, VR};
use dicom::dicom_value;
use dicom::dictionary_std::{tags, StandardDataDictionary};
use dicom::encoding::TransferSyntaxIndex;
use dicom::object::{FileMetaTableBuilder, InMemDicomObject};
use dicom::transfer_syntax::TransferSyntaxRegistry;
use dicom::ul::association::server::AcceptAny;
use dicom::ul::pdu::PDataValueType;
use dicom::ul::{Pdu, ServerAssociationOptions};
use opentelemetry::trace::TraceContextExt;
use opentelemetry::KeyValue;
use uuid::Uuid;

use crate::association_error::{AssociationError, AssociationError::*};
use crate::dicomrs_options::{ClientAETitle, OurAETitle};
use crate::event::AssociationEvent;

/// Handle an "association" from an "SCU" (i.e. handle when someone is trying to give us DICOM files).
///
/// The `uuid` parameter should be a unique UUID for this SCU stream instance.
/// When the association is first established, a [AssociationEvent::Start] event will be sent through `channel`.
/// For each received DICOM file, it will be sent through the `channel` as [AssociationEvent::DicomInstance].
pub fn handle_association(
    scu_stream: TcpStream,
    options: &ServerAssociationOptions<AcceptAny>,
    max_pdu_length: usize,
    channel: &Sender<AssociationEvent>,
    uuid: Uuid,
    aet: &OurAETitle,
    pacs_address: Option<SocketAddrV4>,
) -> Result<(), AssociationError> {
    let mut association = options.establish(scu_stream).map_err(CouldNotEstablish)?;
    let context = opentelemetry::Context::current();
    let aec = association.client_ae_title();
    context
        .span()
        .set_attribute(KeyValue::new("aet", aec.to_string()));
    channel
        .send(AssociationEvent::Start {
            uuid,
            aet: aet.clone(),
            aec: ClientAETitle::from(aec),
            pacs_address,
        })
        .unwrap();

    // tracing::debug!(
    //     "> Presentation contexts: {:?}",
    //     association.presentation_contexts()
    // );

    let mut buffer: Vec<u8> = Vec::with_capacity(max_pdu_length);
    let mut instance_buffer: Vec<u8> = Vec::with_capacity(1024 * 1024);
    let mut msgid = 1;
    let mut sop_class_uid = "".to_string();
    let mut sop_instance_uid = "".to_string();

    while let Some(mut pdu) = bubble_no_pdu(association.receive())? {
        tracing::trace!("scu ----> scp: {}", pdu.short_description());
        match pdu {
            Pdu::PData { ref mut data } => {
                if data.is_empty() {
                    tracing::debug!("Ignoring empty PData PDU");
                    continue;
                }

                if data[0].value_type == PDataValueType::Data && !data[0].is_last {
                    instance_buffer.append(&mut data[0].data);
                } else if data[0].value_type == PDataValueType::Command && data[0].is_last {
                    // commands are always in implict VR LE
                    let ts = dicom::transfer_syntax::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased();
                    let data_value = &data[0];
                    let v = &data_value.data;

                    let obj = InMemDicomObject::read_dataset_with_ts(v.as_slice(), &ts)
                        .map_err(FailedToReadCommand)?;
                    let command_field = obj
                        .element(tags::COMMAND_FIELD)
                        .map_err(|_| MissingTag(tags::COMMAND_FIELD))?
                        .uint16()
                        .map_err(|_| InvalidNumber(tags::COMMAND_FIELD))?;

                    if command_field == 0x0030 {
                        // Handle C-ECHO-RQ
                        let cecho_response = create_cecho_response(msgid);
                        let mut cecho_data = Vec::new();

                        cecho_response
                            .write_dataset_with_ts(&mut cecho_data, &ts)
                            .map_err(|_| CannotRespond("Could not write C-ECHO response object"))?;

                        let pdu_response = Pdu::PData {
                            data: vec![dicom::ul::pdu::PDataValue {
                                presentation_context_id: data[0].presentation_context_id,
                                value_type: PDataValueType::Command,
                                is_last: true,
                                data: cecho_data,
                            }],
                        };
                        association.send(&pdu_response).map_err(|_| {
                            CannotRespond("failed to send C-ECHO response object to SCU")
                        })?;
                    } else {
                        msgid = obj
                            .element(tags::MESSAGE_ID)
                            .map_err(|_| MissingTag(tags::MESSAGE_ID))?
                            .to_int()
                            .map_err(|_| InvalidNumber(tags::MESSAGE_ID))?;
                        sop_class_uid = obj
                            .element(tags::AFFECTED_SOP_CLASS_UID)
                            .map_err(|_| MissingTag(tags::AFFECTED_SOP_CLASS_UID))?
                            .to_str()
                            .map_err(|_| CouldNotRetrieve(tags::AFFECTED_SOP_CLASS_UID))?
                            .to_string();
                        sop_instance_uid = obj
                            .element(tags::AFFECTED_SOP_INSTANCE_UID)
                            .map_err(|_| MissingTag(tags::AFFECTED_SOP_INSTANCE_UID))?
                            .to_str()
                            .map_err(|_| CouldNotRetrieve(tags::AFFECTED_SOP_INSTANCE_UID))?
                            .to_string();
                    }
                    instance_buffer.clear();
                } else if data[0].value_type == PDataValueType::Data && data[0].is_last {
                    instance_buffer.append(&mut data[0].data);

                    let presentation_context = association
                        .presentation_contexts()
                        .iter()
                        .find(|pc| pc.id == data[0].presentation_context_id)
                        .ok_or(MissingPresentationContext)?;
                    let ts = &presentation_context.transfer_syntax;

                    let obj = InMemDicomObject::read_dataset_with_ts(
                        instance_buffer.as_slice(),
                        TransferSyntaxRegistry.get(ts).unwrap(),
                    )
                    .map_err(FailedToReadObject)?;
                    let file_meta = FileMetaTableBuilder::new()
                        .media_storage_sop_class_uid(
                            obj.element(tags::SOP_CLASS_UID)
                                .map_err(|_| MissingTag(tags::SOP_CLASS_UID))?
                                .to_str()
                                .map_err(|_| CouldNotRetrieve(tags::SOP_CLASS_UID))?,
                        )
                        .media_storage_sop_instance_uid(
                            obj.element(tags::SOP_INSTANCE_UID)
                                .map_err(|_| MissingTag(tags::SOP_INSTANCE_UID))?
                                .to_str()
                                .map_err(|_| CouldNotRetrieve(tags::SOP_INSTANCE_UID))?,
                        )
                        .transfer_syntax(ts)
                        .build()
                        .map_err(FailedToBuildMeta)?;

                    // CALL TO ChRIS-RELATED CODE
                    // --------------------------------------------------------------------------------
                    let file_obj = obj.with_exact_meta(file_meta);
                    channel
                        .send(AssociationEvent::DicomInstance {
                            uuid,
                            dcm: file_obj,
                        })
                        .unwrap();
                    // END OF ChRIS-RELATED CODE
                    // --------------------------------------------------------------------------------

                    // send C-STORE-RSP object
                    // commands are always in implict VR LE
                    let ts = dicom::transfer_syntax::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased();

                    let obj = create_cstore_response(msgid, &sop_class_uid, &sop_instance_uid);

                    let mut obj_data = Vec::new();

                    obj.write_dataset_with_ts(&mut obj_data, &ts)
                        .map_err(|_| CannotRespond("could not write response object"))?;

                    let pdu_response = Pdu::PData {
                        data: vec![dicom::ul::pdu::PDataValue {
                            presentation_context_id: data[0].presentation_context_id,
                            value_type: PDataValueType::Command,
                            is_last: true,
                            data: obj_data,
                        }],
                    };
                    association
                        .send(&pdu_response)
                        .map_err(|_| CannotRespond("failed to send response object to SCU"))?;
                }
            }
            Pdu::ReleaseRQ => {
                buffer.clear();
                association.send(&Pdu::ReleaseRP).unwrap_or_else(|e| {
                    let a = vec![KeyValue::new("error", e.to_string())];
                    context
                        .span()
                        .add_event("failed_to_send_association_release", a);
                });
                tracing::info!(
                    "Released association with {}",
                    association.client_ae_title()
                );
            }
            Pdu::AbortRQ { source } => {
                return Err(Aborted(source));
            }
            _ => return Err(UnhandledPdu(pdu.short_description())),
        }
    }
    tracing::info!("Dropping connection with {}", association.client_ae_title());
    Ok(())
}

fn create_cstore_response(
    message_id: u16,
    sop_class_uid: &str,
    sop_instance_uid: &str,
) -> InMemDicomObject<StandardDataDictionary> {
    InMemDicomObject::command_from_element_iter([
        DataElement::new(
            tags::AFFECTED_SOP_CLASS_UID,
            VR::UI,
            dicom_value!(Str, sop_class_uid),
        ),
        DataElement::new(tags::COMMAND_FIELD, VR::US, dicom_value!(U16, [0x8001])),
        DataElement::new(
            tags::MESSAGE_ID_BEING_RESPONDED_TO,
            VR::US,
            dicom_value!(U16, [message_id]),
        ),
        DataElement::new(
            tags::COMMAND_DATA_SET_TYPE,
            VR::US,
            dicom_value!(U16, [0x0101]),
        ),
        DataElement::new(tags::STATUS, VR::US, dicom_value!(U16, [0x0000])),
        DataElement::new(
            tags::AFFECTED_SOP_INSTANCE_UID,
            VR::UI,
            dicom_value!(Str, sop_instance_uid),
        ),
    ])
}

fn create_cecho_response(message_id: u16) -> InMemDicomObject<StandardDataDictionary> {
    InMemDicomObject::command_from_element_iter([
        DataElement::new(tags::COMMAND_FIELD, VR::US, dicom_value!(U16, [0x8030])),
        DataElement::new(
            tags::MESSAGE_ID_BEING_RESPONDED_TO,
            VR::US,
            dicom_value!(U16, [message_id]),
        ),
        DataElement::new(
            tags::COMMAND_DATA_SET_TYPE,
            VR::US,
            dicom_value!(U16, [0x0101]),
        ),
        DataElement::new(tags::STATUS, VR::US, dicom_value!(U16, [0x0000])),
    ])
}

/// Returns `None` if source is [dicom::ul::pdu::reader::Error::NoPduAvailable]
fn bubble_no_pdu(
    pdu: Result<Pdu, dicom::ul::association::server::Error>,
) -> Result<Option<Pdu>, dicom::ul::association::server::Error> {
    pdu.map(Some).or_else(|e| {
        if let dicom::ul::association::server::Error::Receive { source } = &e {
            if matches!(source, dicom::ul::pdu::reader::Error::NoPduAvailable { .. }) {
                Ok(None)
            } else {
                Err(e)
            }
        } else {
            Err(e)
        }
    })
}
