/// Parse DICOM PatientAge to number of days.
///
/// https://github.com/FNNDSC/pypx/blob/7b83154d7c6d631d81eac8c9c4a2fc164ccc2ebc/pypx/register.py#L459-L465
pub(crate) fn parse_age(age: &str) -> Option<u32> {
    for (suffix, coef) in &MULTIPLIERS {
        if let Some(left) = age.strip_suffix(suffix) {
            return left
                .parse::<f32>()
                .map(|num| (num * coef).round() as u32)
                .ok();
        }
    }
    None
}

/// Days per unit of time
const MULTIPLIERS: [(&str, f32); 4] = [("D", 1.0), ("W", 7.0), ("M", 30.44), ("Y", 365.24)];

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    #[rstest]
    #[case("030Y", 10957)]
    #[case("020D", 20)]
    #[case("2W", 14)]
    #[case("5M", 152)]
    fn test_parse_age(#[case] age: &str, #[case] expected: u32) {
        assert_eq!(parse_age(age).unwrap(), expected)
    }
}
