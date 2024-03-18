use std::net::SocketAddrV4;

use dicom::object::DefaultDicomObject;
use uuid::Uuid;

use crate::dicomrs_options::{ClientAETitle, OurAETitle};

/// Events which occur during an association.
pub(crate) enum AssociationEvent {
    /// Association established successfully and client AE title is known.
    Start {
        /// UUID uniquely identifying the TCP connection instance
        uuid: Uuid,
        /// AE title of the client sending us DICOMs
        aec: ClientAETitle,
        /// Our AE title
        aet: OurAETitle,
        /// Address of the client sending us DICOMs
        pacs_address: Option<SocketAddrV4>,
    },
    /// Received a DICOM file.
    DicomInstance { uuid: Uuid, dcm: DefaultDicomObject },
    /// No more DICOM files will be received for this association.
    Finish { uuid: Uuid, ok: bool },
}
