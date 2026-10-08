use bace_economy::{VendorStock, VendorStockError, VendorStockItem, VendorStockReceipt};
fn item(value: &str) -> VendorStockItem {
    let f: Vec<_> = value.split(':').collect();
    VendorStockItem {
        id: f[0].parse().unwrap(),
        template: f[1].parse().unwrap(),
        stack: (f[2] != "-1").then(|| f[2].parse().unwrap()),
        maximum_stack: (f[3] != "-1").then(|| f[3].parse().unwrap()),
    }
}
#[test]
fn original_ace_default_stock_oracle_and_actual_identity_receipts() {
    for line in include_str!("fixtures/vendor_stock.csv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let f: Vec<_> = line.split('|').collect();
        let mut stock = VendorStock::new(16).unwrap();
        for input in f[1].split(';') {
            let incoming = item(input);
            let proposal = stock.propose_default(incoming).unwrap();
            let receipt = stock.commit_default(proposal).unwrap();
            assert!(stock.items().iter().any(|i| i.id == receipt.retained_id()));
            if let Some(retired) = receipt.retired_id() {
                assert_eq!(retired, incoming.id);
                assert!(!stock.items().iter().any(|i| i.id == retired));
            }
        }
        assert_eq!(
            stock.items(),
            f[2].split(';').map(item).collect::<Vec<_>>(),
            "{}",
            f[0]
        );
    }
}
#[test]
fn stale_proposals_and_capacity_fail_without_partial_stock() {
    let mut stock = VendorStock::new(1).unwrap();
    let first = stock.propose_default(item("1:100:1:3")).unwrap();
    let stale = stock.propose_default(item("2:100:1:3")).unwrap();
    stock.commit_default(first).unwrap();
    assert_eq!(stock.commit_default(stale), Err(VendorStockError::Stale));
    let merge = stock.propose_default(item("3:100:3:3")).unwrap();
    assert_eq!(
        stock.commit_default(merge).unwrap(),
        VendorStockReceipt::Stacked {
            retained_id: 1,
            incoming_id: 3,
            stack: 2
        }
    );
    assert!(matches!(
        stock.propose_default(item("4:200:1:3")),
        Err(VendorStockError::Capacity)
    ));
    assert_eq!(stock.items(), &[item("1:100:2:3")]);
    assert_eq!(stock.remove(1, 1), Err(VendorStockError::Stale));
    assert_eq!(stock.remove(1, 2).unwrap(), item("1:100:2:3"));
}

#[test]
fn withdrawal_preserves_other_generator_contributions() {
    use bace_economy::VendorStockWithdrawal;
    let mut stock = VendorStock::new(2).unwrap();
    let item = VendorStockItem {
        id: 10,
        template: 1,
        stack: Some(3),
        maximum_stack: Some(10),
    };
    let proposal = stock.propose_default(item).unwrap();
    stock.commit_default(proposal).unwrap();
    let proposal = stock
        .propose_default(VendorStockItem { id: 11, ..item })
        .unwrap();
    stock.commit_default(proposal).unwrap();
    assert_eq!(
        stock.withdraw_default(10, 3, 2).unwrap(),
        VendorStockWithdrawal::Remaining { id: 10, stack: 1 }
    );
    assert_eq!(
        stock.withdraw_default(10, 1, 2),
        Err(VendorStockError::Stale)
    );
    assert_eq!(
        stock.withdraw_default(10, 2, 3),
        Err(VendorStockError::Invalid)
    );
    assert_eq!(
        stock.withdraw_default(10, 1, 3).unwrap(),
        VendorStockWithdrawal::Removed {
            item: VendorStockItem {
                stack: Some(1),
                ..item
            }
        }
    );
    assert!(stock.items().is_empty());
}

#[test]
fn lazy_shop_keeps_duplicate_source_rows_and_rejects_stale_batch() {
    // Pinned Vendor.LoadInventory calls LoadInventoryItem for each Shop row;
    // it appends to DefaultItemsForSale without AddDefaultItem's WCID merge.
    let mut stock = VendorStock::new(2).unwrap();
    let first = stock.propose_lazy(item("20:100:1:3")).unwrap();
    let stale = stock.propose_lazy(item("21:100:1:3")).unwrap();
    assert_eq!(stock.commit_lazy(first).unwrap(), 20);
    assert_eq!(stock.commit_lazy(stale), Err(VendorStockError::Stale));
    let second = stock.propose_lazy(item("21:100:1:3")).unwrap();
    assert_eq!(stock.commit_lazy(second).unwrap(), 21);
    assert_eq!(stock.items(), &[item("20:100:1:3"), item("21:100:1:3")]);
    assert!(matches!(
        stock.propose_lazy(item("22:100:1:3")),
        Err(VendorStockError::Capacity)
    ));
}
