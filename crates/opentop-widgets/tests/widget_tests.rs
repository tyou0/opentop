use opentop_widgets::processes::ProcessSort;

#[test]
fn test_process_sort_cycle() {
    let sort = ProcessSort::Cpu;
    let next = sort.cycle();
    assert_ne!(sort, next);
    let back = next.cycle().cycle().cycle();
    assert_eq!(sort, back);
}
