use dicom::dictionary_std::uids;
use dicom::transfer_syntax::TransferSyntaxRegistry;
use dicom::ul::association::server::AcceptAny;
use dicom::ul::ServerAssociationOptions;
use crate::transfer::ABSTRACT_SYNTAXES;

pub struct DicomRsConfig {
    pub calling_ae_title: String,
    /// Whether receiving PDUs must not surpass the negotiated maximum PDU length.
    pub strict: bool,
    pub uncompressed_only: bool,
}

impl<'a> Into<ServerAssociationOptions<'a, AcceptAny>> for DicomRsConfig {
    fn into(self) -> ServerAssociationOptions<'a, AcceptAny> {
        let mut options = dicom::ul::association::ServerAssociationOptions::new()
            .accept_any()
            .ae_title(self.calling_ae_title)
            .strict(self.strict);
        if self.uncompressed_only {
            options = options
                .with_transfer_syntax(uids::IMPLICIT_VR_LITTLE_ENDIAN)
                .with_transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN);
        } else {
            for ts in TransferSyntaxRegistry.iter() {
                if !ts.is_unsupported() {
                    options = options.with_transfer_syntax(ts.uid());
                }
            }
        };
        for uid in ABSTRACT_SYNTAXES {
            options = options.with_abstract_syntax(*uid);
        };
        options
    }
}
