use core::iter;

use super::Validator;
use crate::packaging::package::Package;
use crate::validation::error::{ErrorCode, ValidationError};
use crate::xml::name::OfficeVersion;

fn errors(count: usize) -> Vec<ValidationError> {
    iter::repeat_with(|| ValidationError::new(ErrorCode::ChoiceWithoutRequires, None, None, &[]))
        .take(count)
        .collect()
}

#[test]
fn a_new_validator_stops_at_1000_errors() {
    let validator = Validator::new(OfficeVersion::Office2016);
    assert_eq!(Validator::DEFAULT_MAX_ERRORS, 1000);
    assert_eq!(validator.max_errors(), 1000);
    assert_eq!(validator.version(), OfficeVersion::Office2016);
}

#[test]
fn the_error_that_reaches_the_cap_is_kept() {
    let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(3);
    assert_eq!(validator.limit(errors(2)).len(), 2);
    assert_eq!(validator.limit(errors(3)).len(), 3);
    assert_eq!(validator.limit(errors(4)).len(), 3);
}

#[test]
fn zero_means_no_limit() {
    let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(0);
    assert_eq!(validator.limit(errors(2500)).len(), 2500);
}

#[test]
fn no_pass_runs_yet_so_a_package_has_no_errors() {
    let validator = Validator::new(OfficeVersion::Microsoft365);
    assert_eq!(validator.validate(&Package::new()).len(), 0);
}
