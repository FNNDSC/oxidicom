use dicom::core::{DataElement, VR};
use dicom::dictionary_std::{tags, uids};
use dicom::object::InMemDicomObject;
use oxidicom::{AETitle, SeriesKey};
use std::sync::LazyLock;

pub(crate) static SERIES: LazyLock<SeriesKey> = LazyLock::new(|| {
    SeriesKey::new(
        "2.25.281556350530040985498456895882693555497".to_string(),
        AETitle::from_static("OXITESTCALLING"),
        ulid::Ulid::new(),
    )
});

pub(crate) fn create_dicom_without_studydate() -> InMemDicomObject {
    InMemDicomObject::from_element_iter([
        DataElement::new(
            tags::SOP_CLASS_UID,
            VR::UI,
            uids::COMPUTED_RADIOGRAPHY_IMAGE_STORAGE,
        ),
        DataElement::new(
            tags::STUDY_INSTANCE_UID,
            VR::UI,
            "2.25.127942697262855382468303288367206048762",
        ),
        DataElement::new(
            tags::SERIES_INSTANCE_UID,
            VR::UI,
            SERIES.SeriesInstanceUID.as_str(),
        ),
        DataElement::new(
            tags::SOP_INSTANCE_UID,
            VR::UI,
            "2.25.164452200898186296452633608713549770669",
        ),
        DataElement::new(tags::PATIENT_ID, VR::LO, "123ABC"),
    ])
}
