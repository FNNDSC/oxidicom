use crate::assertions::model::SeriesParams;
use std::collections::HashSet;
use std::sync::LazyLock;

pub static EXPECTED_SERIES: LazyLock<HashSet<SeriesParams>> = LazyLock::new(|| {
    HashSet::from([
        SeriesParams {
            PatientID: "1449c1d".to_string(),
            StudyDate: "2013-03-08".to_string(),
            StudyInstanceUID: "1.2.840.113845.11.1000000001785349915.20130308061609.6346698".to_string(),
            SeriesInstanceUID: "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0".to_string(),
            pacs_name: "OXITESTORTHANC".to_string(),
            path: "SERVICES/PACS/OXITESTORTHANC/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/00005-SAG_MPRAGE_220_FOV-a27cf06".to_string(),
            ndicom: 192,
            PatientName: Some("anonymized".to_string()),
            PatientBirthDate: Some("20090701".to_string()),
            PatientAge: Some(1096, ),
            PatientSex: Some("M".to_string()),
            AccessionNumber: Some("98edede8b2".to_string()),
            Modality: Some("MR".to_string()),
            ProtocolName: Some("SAG MPRAGE 220 FOV".to_string()),
            StudyDescription: Some("MR-Brain w/o Contrast".to_string()),
            SeriesDescription: Some("SAG MPRAGE 220 FOV".to_string())
        },
        SeriesParams {
            PatientID: "02".to_string(),
            StudyDate: "2013-07-17".to_string(),
            StudyInstanceUID: "1.2.826.0.1.3680043.2.1143.2592092611698916978113112155415165916".to_string(),
            SeriesInstanceUID: "1.2.826.0.1.3680043.2.1143.515404396022363061013111326823367652".to_string(),
            pacs_name: "OXITESTORTHANC".to_string(),
            path: "SERVICES/PACS/OXITESTORTHANC/02-Jane_Doe-19660101/Hanke_Stadler_0024_transrep-AccessionNumber-20130717/00401-anat-T1w-661b8fc".to_string(),
            ndicom: 384,
            PatientName: Some("Jane_Doe".to_string()),
            PatientBirthDate: Some("19660101".to_string()),
            PatientAge: None,
            PatientSex: Some("F".to_string()),
            AccessionNumber: None,
            Modality: Some("MR".to_string()),
            ProtocolName: Some("anat-T1w".to_string()),
            StudyDescription: Some("Hanke_Stadler^0024_transrep".to_string()),
            SeriesDescription: Some("anat-T1w".to_string()),
        },
    ])
});
