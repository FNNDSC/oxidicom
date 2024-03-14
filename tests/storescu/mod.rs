use std::collections::HashSet;
use std::io::Write;
use std::thread;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use dicom::core::{DataElement, Tag, VR};
use dicom::dicom_value;
use dicom::dictionary_std::{tags, uids, StandardDataDictionary};
use dicom::encoding::TransferSyntaxIndex;
use dicom::object::{open_file, InMemDicomObject};
use dicom::transfer_syntax::TransferSyntaxRegistry;
use dicom::ul::pdu::{PDataValue, PDataValueType};
use dicom::ul::{ClientAssociationOptions, Pdu};
use snafu::{Report, ResultExt, Snafu};

/// Push files to a listener.
///
/// Based on
/// https://github.com/Enet4/dicom-rs/blob/dbd41ed3a0d1536747c6b8ea2b286e4c6e8ccc8a/storescu/src/main.rs
pub fn dicom_client(data_dir: Utf8PathBuf) {
    run(&data_dir).unwrap()
}

pub fn get_test_files(data_dir: &Utf8Path) -> Vec<Utf8PathBuf> {
    let files: Vec<_> = walkdir::WalkDir::new(data_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|e| {
            if !e.metadata().unwrap().is_file() {
                return None;
            }
            let path = Utf8PathBuf::from_path_buf(e.into_path()).unwrap();
            if path.extension().map(|s| s == "dcm").unwrap_or(false) {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    assert!(!files.is_empty());
    files
}

fn run(data_dir: &Utf8Path) -> Result<(), Error> {
    thread::sleep(Duration::from_secs(1)); // wait for server to start
    let port = envmnt::get_i16("PORT", 11112);
    let addr = format!("127.0.0.1:{port}");
    let never_transcode = false;
    let calling_ae_title = "STORE-SCU-TEST";
    let called_ae_title = "ChRISTEST";
    let max_pdu_length = 16384;
    let message_id = 1;

    let checked_files = get_test_files(data_dir);

    let mut dicom_files: Vec<DicomFile> = vec![];
    let mut presentation_contexts = HashSet::new();

    for file in checked_files {
        match check_file(&file) {
            Ok(dicom_file) => {
                presentation_contexts.insert((
                    dicom_file.sop_class_uid.to_string(),
                    dicom_file.file_transfer_syntax.clone(),
                ));

                // also accept uncompressed transfer syntaxes
                // as mandated by the standard
                // (though it might not always be able to fulfill this)
                if !never_transcode {
                    presentation_contexts.insert((
                        dicom_file.sop_class_uid.to_string(),
                        uids::EXPLICIT_VR_LITTLE_ENDIAN.to_string(),
                    ));
                    presentation_contexts.insert((
                        dicom_file.sop_class_uid.to_string(),
                        uids::IMPLICIT_VR_LITTLE_ENDIAN.to_string(),
                    ));
                }

                dicom_files.push(dicom_file);
            }
            Err(_) => {
                panic!("Could not open file {} as DICOM", file);
            }
        }
    }

    if dicom_files.is_empty() {
        panic!("No supported files to transfer");
    }

    let mut scu_init = ClientAssociationOptions::new()
        .calling_ae_title(calling_ae_title)
        .max_pdu_length(max_pdu_length);

    for (storage_sop_class_uid, transfer_syntax) in &presentation_contexts {
        scu_init = scu_init.with_presentation_context(storage_sop_class_uid, vec![transfer_syntax]);
    }

    scu_init = scu_init.called_ae_title(called_ae_title);

    let mut scu = scu_init.establish_with(&addr).context(InitScuSnafu)?;

    for file in &mut dicom_files {
        // identify the right transfer syntax to use
        let r: Result<_, Error> =
            check_presentation_contexts(file, scu.presentation_contexts(), never_transcode)
                .whatever_context::<_, _>("Could not choose a transfer syntax");
        match r {
            Ok((pc, ts)) => {
                file.pc_selected = Some(pc);
                file.ts_selected = Some(ts);
            }
            Err(e) => {
                panic!("{}", Report::from_error(e));
            }
        }
    }

    for file in dicom_files {
        if let (Some(pc_selected), Some(ts_uid_selected)) = (file.pc_selected, file.ts_selected) {
            let cmd = store_req_command(&file.sop_class_uid, &file.sop_instance_uid, message_id);

            let mut cmd_data = Vec::with_capacity(128);
            cmd.write_dataset_with_ts(
                &mut cmd_data,
                &dicom::transfer_syntax::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased(),
            )
            .context(CreateCommandSnafu)?;

            let mut object_data = Vec::with_capacity(2048);
            let dicom_file =
                open_file(&file.file).whatever_context("Could not open listed DICOM file")?;
            let ts_selected = TransferSyntaxRegistry.get(&ts_uid_selected).unwrap();
            // idk why this doesn't compile
            // .with_context(|| UnsupportedFileTransferSyntaxSnafu { uid: ts_uid_selected.to_string() })?;

            // transcode file if necessary
            // do not call this function, see https://github.com/Enet4/dicom-rs/issues/473
            // let dicom_file = into_ts(dicom_file, ts_selected)?;

            dicom_file
                .write_dataset_with_ts(&mut object_data, ts_selected)
                .whatever_context("Could not write object dataset")?;

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

                scu.send(&pdu)
                    .whatever_context("Failed to send C-STORE-RQ")?;
            } else {
                let pdu = Pdu::PData {
                    data: vec![PDataValue {
                        presentation_context_id: pc_selected.id,
                        value_type: PDataValueType::Command,
                        is_last: true,
                        data: cmd_data,
                    }],
                };

                scu.send(&pdu)
                    .whatever_context("Failed to send C-STORE-RQ command")?;

                {
                    let mut pdata = scu.send_pdata(pc_selected.id);
                    pdata
                        .write_all(&object_data)
                        .whatever_context("Failed to send C-STORE-RQ P-Data")?;
                }
            }

            let rsp_pdu = scu
                .receive()
                .whatever_context("Failed to receive C-STORE-RSP")?;

            match rsp_pdu {
                Pdu::PData { data } => {
                    let data_value = &data[0];

                    let cmd_obj = InMemDicomObject::read_dataset_with_ts(
                        &data_value.data[..],
                        &dicom::transfer_syntax::entries::IMPLICIT_VR_LITTLE_ENDIAN.erased(),
                    )
                    .whatever_context("Could not read response from SCP")?;
                    let status = cmd_obj
                        .element(tags::STATUS)
                        .whatever_context("Could not find status code in response")?
                        .to_int::<u16>()
                        .whatever_context("Status code in response is not a valid integer")?;
                    let storage_sop_instance_uid = file
                        .sop_instance_uid
                        .trim_end_matches(|c: char| c.is_whitespace() || c == '\0');

                    match status {
                        // Success
                        0 => {}
                        // Warning
                        1 | 0x0107 | 0x0116 | 0xB000..=0xBFFF => {
                            panic!(
                                "Possible issue storing instance `{}` (status code {:04X}H)",
                                storage_sop_instance_uid, status
                            );
                        }
                        0xFF00 | 0xFF01 => {
                            panic!(
                                "Possible issue storing instance `{}`: status is pending (status code {:04X}H)",
                                storage_sop_instance_uid, status
                            );
                        }
                        0xFE00 => {
                            panic!(
                                "Could not store instance `{}`: operation cancelled",
                                storage_sop_instance_uid
                            );
                        }
                        _ => {
                            panic!(
                                "Failed to store instance `{}` (status code {:04X}H)",
                                storage_sop_instance_uid, status
                            );
                        }
                    }
                }

                pdu @ Pdu::Unknown { .. }
                | pdu @ Pdu::AssociationRQ { .. }
                | pdu @ Pdu::AssociationAC { .. }
                | pdu @ Pdu::AssociationRJ { .. }
                | pdu @ Pdu::ReleaseRQ
                | pdu @ Pdu::ReleaseRP
                | pdu @ Pdu::AbortRQ { .. } => {
                    panic!("Unexpected SCP response: {:?}", pdu);
                }
            }
            eprintln!("Client sent {}", &file.file);
        }
    }

    scu.release()
        .whatever_context("Failed to release SCU association")?;
    Ok(())
}

struct DicomFile {
    /// File path
    file: Utf8PathBuf,
    /// Storage SOP Class UID
    sop_class_uid: String,
    /// Storage SOP Instance UID
    sop_instance_uid: String,
    /// File Transfer Syntax
    file_transfer_syntax: String,
    /// Transfer Syntax selected
    ts_selected: Option<String>,
    /// Presentation Context selected
    pc_selected: Option<dicom::ul::pdu::PresentationContextResult>,
}

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

fn check_presentation_contexts(
    file: &DicomFile,
    pcs: &[dicom::ul::pdu::PresentationContextResult],
    never_transcode: bool,
) -> Result<(dicom::ul::pdu::PresentationContextResult, String), Error> {
    let file_ts = TransferSyntaxRegistry
        .get(&file.file_transfer_syntax)
        .unwrap();
    // if destination does not support original file TS,
    // check whether we can transcode to explicit VR LE

    let pc = pcs.iter().find(|pc| {
        // Check support for this transfer syntax.
        // If it is the same as the file, we're good.
        // Otherwise, uncompressed data set encoding
        // and native pixel data is required on both ends.
        let ts = &pc.transfer_syntax;
        ts == file_ts.uid()
            || TransferSyntaxRegistry
                .get(&pc.transfer_syntax)
                .filter(|ts| file_ts.is_codec_free() && ts.is_codec_free())
                .map(|_| true)
                .unwrap_or(false)
    });

    let pc = match pc {
        Some(pc) => pc,
        None => {
            if never_transcode || !file_ts.can_decode_all() {
                panic!("No presentation context acceptable");
            }

            // Else, if transcoding is possible, we go for it.
            pcs.iter()
                // accept explicit VR little endian
                .find(|pc| pc.transfer_syntax == uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .or_else(||
                    // accept implicit VR little endian
                    pcs.iter()
                        .find(|pc| pc.transfer_syntax == uids::IMPLICIT_VR_LITTLE_ENDIAN))
                .unwrap()
            // welp
        }
    };
    let ts = TransferSyntaxRegistry.get(&pc.transfer_syntax).unwrap();

    Ok((pc.clone(), String::from(ts.uid())))
}

fn check_file(file: &Utf8Path) -> Result<DicomFile, Error> {
    // Ignore DICOMDIR files until better support is added
    let _ = (file.file_name() != Some("DICOMDIR"))
        .then_some(false)
        .unwrap();
    let dicom_file = dicom::object::OpenFileOptions::new()
        .read_until(Tag(0x0001, 0x000))
        .open_file(file)
        .with_whatever_context(|_| format!("Could not open DICOM file {}", file))?;

    let meta = dicom_file.meta();

    let storage_sop_class_uid = &meta.media_storage_sop_class_uid;
    let storage_sop_instance_uid = &meta.media_storage_sop_instance_uid;
    let transfer_syntax_uid = &meta.transfer_syntax.trim_end_matches('\0');
    let ts = TransferSyntaxRegistry.get(transfer_syntax_uid).unwrap();
    Ok(DicomFile {
        file: file.to_path_buf(),
        sop_class_uid: storage_sop_class_uid.to_string(),
        sop_instance_uid: storage_sop_instance_uid.to_string(),
        file_transfer_syntax: String::from(ts.uid()),
        ts_selected: None,
        pc_selected: None,
    })
}

#[derive(Debug, Snafu)]
enum Error {
    /// Could not initialize SCU
    InitScu {
        source: dicom::ul::association::client::Error,
    },

    /// Could not construct DICOM command
    CreateCommand { source: dicom::object::WriteError },

    /// Unsupported file transfer syntax {uid}
    UnsupportedFileTransferSyntax { uid: std::borrow::Cow<'static, str> },

    #[snafu(whatever, display("{}", message))]
    Other {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error + 'static>, Some)))]
        source: Option<Box<dyn std::error::Error + 'static>>,
    },
}
