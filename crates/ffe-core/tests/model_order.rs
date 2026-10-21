use ffe_core::model_order::Order;

#[test]
fn unsigned_descending_order_breaks_ties_by_earlier_allocation() {
    let mut order = Order::new(8).unwrap();
    for (key, value) in [
        (1, 1),
        (u32::MAX, 2),
        (0, 3),
        (1, 4),
        (0x80000000, 5),
        (u32::MAX, 6),
    ] {
        order.push(key, value).unwrap();
    }
    assert_eq!(order.finish().unwrap(), [2, 6, 5, 1, 4, 3]);
}

#[test]
fn groups_preserve_primitive_order_ignore_member_keys_and_do_not_nest() {
    let mut order = Order::new(12).unwrap();
    order.push(5, 0).unwrap();
    order.group(Some(5)).unwrap();
    order.push(0, 1).unwrap();
    order.push(u32::MAX, 2).unwrap();
    order.group(Some(7)).unwrap();
    order.push(0, 3).unwrap();
    order.group(Some(5)).unwrap();
    order.push(100, 4).unwrap();
    order.group(None).unwrap();
    order.push(5, 5).unwrap();
    assert_eq!(order.finish().unwrap(), [3, 0, 1, 2, 4, 5]);
}

#[test]
fn empty_groups_close_cleanly_and_unterminated_groups_error() {
    let mut order = Order::<u32>::new(4).unwrap();
    order.group(None).unwrap();
    order.group(Some(1)).unwrap();
    assert!(order.clone().finish().is_err());
    order.group(Some(2)).unwrap();
    order.group(None).unwrap();
    assert_eq!(order.finish().unwrap(), []);
}

#[test]
fn exact_operation_budget_rejects_without_mutating_queue_or_active_group() {
    for budget in [0, 4097, usize::MAX] {
        assert!(Order::<u32>::new(budget).is_err());
    }
    let mut order = Order::new(3).unwrap();
    order.group(Some(1)).unwrap();
    order.push(0, 7).unwrap();
    order.group(None).unwrap();
    let before = order.clone();
    assert!(order.push(0, 8).is_err());
    assert_eq!(order, before);
    assert!(order.group(Some(2)).is_err());
    assert_eq!(order, before);
    assert_eq!(order.finish().unwrap(), [7]);
}
