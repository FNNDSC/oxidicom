use crate::transfer::ABSTRACT_SYNTAXES;
use aliri_braid::braid;
use dicom::dictionary_std::uids;
use dicom::transfer_syntax::TransferSyntaxRegistry;
use dicom::ul::association::server::AcceptAny;
use dicom::ul::ServerAssociationOptions;

/// Our AE title.
#[braid(serde)]
pub struct OurAETitle;

/// The AE title of a peer PACS server pushing DICOMs to us.
#[braid(serde)]
pub struct ClientAETitle;

#[derive(Debug, serde::Deserialize)]
pub struct DicomRsSettings {
    /// Our AE title.
    #[serde(default = "default_aet")]
    pub aet: OurAETitle,
    /// Whether receiving PDUs must not surpass the negotiated maximum PDU length.
    #[serde(default)]
    pub strict: bool,
    /// Only accept uncompressed transfer syntaxes.
    #[serde(default)]
    pub uncompressed_only: bool,
    /// Whether to accept unknown abstract syntaxes.
    #[serde(default)]
    pub promiscuous: bool,
}

impl<'a> Into<ServerAssociationOptions<'a, AcceptAny>> for DicomRsSettings {
    fn into(self) -> ServerAssociationOptions<'a, AcceptAny> {
        let mut options = dicom::ul::association::ServerAssociationOptions::new()
            .accept_any()
            .ae_title(self.aet.to_string())
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
        }
        options.promiscuous(self.promiscuous)
    }
}

fn default_aet() -> OurAETitle {
    OurAETitle::from_static("ChRIS")
}
