use crate::transfer::ABSTRACT_SYNTAXES;
use aliri_braid::braid;
use dicom::dictionary_std::uids;
use dicom::transfer_syntax::TransferSyntaxRegistry;
use dicom::ul::association::server::AcceptAny;
use dicom::ul::ServerAssociationOptions;

/// The AE title of a peer PACS server pushing DICOMs to us.
#[braid(serde)]
pub struct AETitle;

#[derive(Debug, serde::Deserialize)]
pub struct DicomRsSettings {
    /// Our AE title.
    #[serde(default = "default_aet")]
    pub aet: String,
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

impl<'a> From<DicomRsSettings> for ServerAssociationOptions<'a, AcceptAny> {
    fn from(value: DicomRsSettings) -> Self {
        let mut options = dicom::ul::association::ServerAssociationOptions::new()
            .accept_any()
            .ae_title(value.aet.to_string())
            .strict(value.strict);
        if value.uncompressed_only {
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
        options.promiscuous(value.promiscuous)
    }
}

fn default_aet() -> String {
    "ChRIS".to_string()
}
