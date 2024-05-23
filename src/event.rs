use dicom::object::DefaultDicomObject;
use ulid::Ulid;

use crate::dicomrs_options::{ClientAETitle, OurAETitle};

/// Events which occur during an association.
pub(crate) enum AssociationEvent {
    /// Association established successfully and client AE title is known.
    Start {
        /// UUID uniquely identifying the TCP connection instance
        ulid: Ulid,
        /// AE title of the client sending us DICOMs
        aec: ClientAETitle,
        /// Our AE title
        aet: OurAETitle,
        /// Address of the client sending us DICOMs
        pacs_address: Option<String>,
    },
    /// Received a DICOM file.
    DicomInstance { ulid: Ulid, dcm: DefaultDicomObject },
    /// No more DICOM files will be received for this association.
    Finish { ulid: Ulid, ok: bool },
}
