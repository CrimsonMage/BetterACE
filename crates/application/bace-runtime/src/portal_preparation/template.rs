//! Source template qualities without a fake entity or a guessed source position.
use bace_content::WeenieV1;
use bace_interactions::{PortalError, PortalKind, PortalTemplate};
pub fn prepare_portal_template(template: &WeenieV1) -> Result<PortalTemplate, PortalError> {
    let integer = |id, default| {
        template
            .properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(Ok(default), |p| {
                u32::try_from(p.value).map_err(|_| PortalError::Invalid)
            })
    };
    let kind = match template.weenie_type {
        7 => PortalKind::Portal,
        25 => PortalKind::Lifestone,
        _ => return Err(PortalError::WrongKind),
    };
    let restrictions = integer(111, 0)?;
    let result = PortalTemplate {
        template: template.weenie_id,
        original_template: template
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 49 && p.value != 0)
            .map(|p| p.value),
        kind,
        destination: template
            .properties
            .positions
            .iter()
            .find(|p| p.id == 2)
            .map(|p| super::position(&p.value)),
        minimum_level: integer(86, 0)?,
        maximum_level: integer(87, 0)?,
        restrictions,
        no_tie: restrictions & 0x20 != 0,
        ignore_pk_timer: template
            .properties
            .bools
            .iter()
            .any(|p| p.id == 89 && p.value),
        account_requirement: integer(26, 0)?,
    };
    result.validate()?;
    Ok(result)
}
