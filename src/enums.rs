use dicom::object::DefaultDicomObject;
use ulid::Ulid;

use crate::AETitle;

/// Events which occur during an association.
pub(crate) enum AssociationEvent {
    /// Association established successfully and client AE title is known.
    Start {
        /// UUID uniquely identifying the TCP connection instance
        ulid: Ulid,
        /// AE title of the client sending us DICOMs
        aec: AETitle,
    },
    /// Received a DICOM file.
    DicomInstance {
        /// UUID uniquely identifying the TCP connection instance
        ulid: Ulid,
        /// DICOM data
        dcm: DefaultDicomObject,
    },
    /// No more DICOM files will be received for this association.
    Finish {
        /// ULID of the association
        ulid: Ulid,
        /// Whether there was an error with the association
        #[allow(unused)]
        ok: bool,
    },
}

/// An event which occurs during the reception of a DICOM series.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum SeriesEvent<T, F> {
    /// DICOM instance received for a series.
    Instance(T),
    /// No more DICOM data will be received for the series.
    Finish(F),
}
