use crate::util::dicom_wo_studydate::SERIES;
use dicom::core::{DataElement, VR};
use dicom::dicom_value;
use dicom::dictionary_std::{tags, uids};
use dicom::object::{InMemDicomObject, StandardDataDictionary};
use dicom::transfer_syntax::entries;
use dicom_ul::pdu::{PDataValue, PDataValueType};
use dicom_ul::{ClientAssociation, ClientAssociationOptions, Pdu};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

pub(crate) async fn store_one_dicom(addr: &str, dcm: InMemDicomObject) {
    let scu = ClientAssociationOptions::new()
        .calling_ae_title(SERIES.pacs_name.as_str())
        .called_ae_title("OXIDICOMTEST")
        .max_pdu_length(16384)
        .with_presentation_context(
            uids::COMPUTED_RADIOGRAPHY_IMAGE_STORAGE,
            vec![uids::IMPLICIT_VR_LITTLE_ENDIAN],
        )
        .establish_with_async(addr)
        .await
        .unwrap();
    let scu = send_dicom(scu, dcm).await;
    scu.release().await.unwrap();
}

/// Copied from
/// https://github.com/Enet4/dicom-rs/blob/801553a2112950930d98ac20c331810461629990/storescu/src/store_async.rs
pub async fn send_dicom(
    mut scu: ClientAssociation<TcpStream>,
    dcm: InMemDicomObject,
) -> ClientAssociation<TcpStream> {
    let sop_class_uid = uids::COMPUTED_RADIOGRAPHY_IMAGE_STORAGE;
    let sop_instance_uid = dcm
        .element(tags::SOP_INSTANCE_UID)
        .unwrap()
        .string()
        .unwrap();
    let message_id = 1;
    let pc_selected = scu
        .presentation_contexts()
        .iter()
        .filter(|pc| pc.transfer_syntax == uids::IMPLICIT_VR_LITTLE_ENDIAN)
        .next()
        .unwrap()
        .clone();
    let cmd = store_req_command(sop_class_uid, sop_instance_uid, message_id);

    let mut cmd_data = Vec::with_capacity(128);
    cmd.write_dataset_with_ts(&mut cmd_data, &entries::IMPLICIT_VR_LITTLE_ENDIAN.erased())
        .unwrap();
    let mut object_data = Vec::with_capacity(2048);
    dcm.write_dataset_with_ts(
        &mut object_data,
        &entries::IMPLICIT_VR_LITTLE_ENDIAN.erased(),
    )
    .unwrap();

    let nbytes = cmd_data.len() + object_data.len();

    if nbytes < scu.acceptor_max_pdu_length().saturating_sub(100) as usize {
        let pdu = Pdu::PData {
            data: vec![
                PDataValue {
                    presentation_context_id: pc_selected.id,
                    value_type: PDataValueType::Command,
                    is_last: true,
                    data: cmd_data,
                },
                PDataValue {
                    presentation_context_id: pc_selected.id,
                    value_type: PDataValueType::Data,
                    is_last: true,
                    data: object_data,
                },
            ],
        };

        scu.send(&pdu).await.unwrap();
    } else {
        let pdu = Pdu::PData {
            data: vec![PDataValue {
                presentation_context_id: pc_selected.id,
                value_type: PDataValueType::Command,
                is_last: true,
                data: cmd_data,
            }],
        };

        scu.send(&pdu).await.unwrap();

        {
            let mut pdata = scu.send_pdata(pc_selected.id).await;
            pdata.write_all(&object_data).await.unwrap();
        }
    }

    let rsp_pdu = scu.receive().await.unwrap();

    match rsp_pdu {
        Pdu::PData { data } => {
            let data_value = &data[0];

            let cmd_obj = InMemDicomObject::read_dataset_with_ts(
                &data_value.data[..],
                &entries::IMPLICIT_VR_LITTLE_ENDIAN.erased(),
            )
            .unwrap();
            let status = cmd_obj
                .element(tags::STATUS)
                .unwrap()
                .to_int::<u16>()
                .unwrap();
            assert_eq!(status, 0);
        }

        pdu @ Pdu::Unknown { .. }
        | pdu @ Pdu::AssociationRQ { .. }
        | pdu @ Pdu::AssociationAC { .. }
        | pdu @ Pdu::AssociationRJ { .. }
        | pdu @ Pdu::ReleaseRQ
        | pdu @ Pdu::ReleaseRP
        | pdu @ Pdu::AbortRQ { .. } => {
            let _ = scu.abort().await;
            panic!("Unexpected SCP response: {:?}", pdu);
        }
    }
    scu
}

/// Copied from
/// https://github.com/Enet4/dicom-rs/blob/801553a2112950930d98ac20c331810461629990/storescu/src/main.rs#L519C1-L551C1
fn store_req_command(
    storage_sop_class_uid: &str,
    storage_sop_instance_uid: &str,
    message_id: u16,
) -> InMemDicomObject<StandardDataDictionary> {
    InMemDicomObject::command_from_element_iter([
        // SOP Class UID
        DataElement::new(
            tags::AFFECTED_SOP_CLASS_UID,
            VR::UI,
            dicom_value!(Str, storage_sop_class_uid),
        ),
        // command field
        DataElement::new(tags::COMMAND_FIELD, VR::US, dicom_value!(U16, [0x0001])),
        // message ID
        DataElement::new(tags::MESSAGE_ID, VR::US, dicom_value!(U16, [message_id])),
        //priority
        DataElement::new(tags::PRIORITY, VR::US, dicom_value!(U16, [0x0000])),
        // data set type
        DataElement::new(
            tags::COMMAND_DATA_SET_TYPE,
            VR::US,
            dicom_value!(U16, [0x0000]),
        ),
        // affected SOP Instance UID
        DataElement::new(
            tags::AFFECTED_SOP_INSTANCE_UID,
            VR::UI,
            dicom_value!(Str, storage_sop_instance_uid),
        ),
    ])
}
