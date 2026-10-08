//! Pinned ACE Vendor.GetBuyCost/GetSellCost, AGPL-3.0-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceError {
    InvalidInput,
    Overflow,
}

fn raw(value: Option<i32>, rate: f64) -> Result<f64, PriceError> {
    let value = value.unwrap_or(0);
    if value < 0 || !rate.is_finite() || rate < 0.0 {
        return Err(PriceError::InvalidInput);
    }
    let product = rate as f32 * value as f32;
    if !product.is_finite() {
        return Err(PriceError::Overflow);
    }
    Ok(f64::from(product))
}
/// What the player pays; promissory notes force the official 1.15 multiplier.
pub fn vendor_sell_cost(
    value: Option<i32>,
    sell_rate: Option<f64>,
    promissory_note: bool,
) -> Result<u32, PriceError> {
    let rate = if promissory_note {
        1.15
    } else {
        sell_rate.unwrap_or(1.0)
    };
    let cost = (raw(value, rate)? - 0.1).ceil().max(1.0);
    if cost > f64::from(u32::MAX) {
        return Err(PriceError::Overflow);
    }
    Ok(cost as u32)
}
/// What the vendor pays; promissory notes force the official 1.0 multiplier.
pub fn vendor_buy_cost(
    value: Option<i32>,
    buy_rate: Option<f64>,
    promissory_note: bool,
) -> Result<i32, PriceError> {
    let rate = if promissory_note {
        1.0
    } else {
        buy_rate.unwrap_or(1.0)
    };
    let cost = (raw(value, rate)? + 0.1).floor().max(1.0);
    if cost > f64::from(i32::MAX) {
        return Err(PriceError::Overflow);
    }
    Ok(cost as i32)
}
