use bace_economy::{vendor_buy_cost, vendor_sell_cost};
#[test]
fn official_vendor_price_vectors() {
    for line in include_str!("fixtures/prices.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        let value = Some(p[0].parse().unwrap());
        let rate = Some(p[1].parse().unwrap());
        let note = p[2] == "1";
        assert_eq!(
            vendor_sell_cost(value, rate, note).unwrap(),
            p[3].parse::<u32>().unwrap(),
            "{line}"
        );
        assert_eq!(
            vendor_buy_cost(value, rate, note).unwrap(),
            p[4].parse::<i32>().unwrap(),
            "{line}"
        );
    }
    assert!(vendor_buy_cost(Some(-1), Some(1.0), false).is_err());
    assert!(vendor_sell_cost(Some(1), Some(f64::NAN), false).is_err());
    assert!(vendor_sell_cost(Some(i32::MAX), Some(3.0), false).is_err());
}
