use dicom::object::DefaultDicomObject;
use tokio::task::JoinHandle;
use ulid::Ulid;

use crate::dicomrs_settings::{ClientAETitle, OurAETitle};
use crate::pacs_file::PacsFileRegistrationRequest;

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

/// A message sent from [crate::association_series_state_loop::association_series_state_loop]
/// to [crate::registration_synchronizer::registration_synchronizer].
pub(crate) enum PendingRegistration {
    /// A task which, if successful, produces a [PacsFileRegistrationRequest] which should
    /// be added to a batch in preparation for registration to the database.
    ///
    /// Error handling should be done by the sender, so the [Err] type is `()`.
    Task(JoinHandle<Result<PacsFileRegistrationRequest, ()>>),
    /// Indicates that no other tasks shall be sent for a given series.
    End,
}
