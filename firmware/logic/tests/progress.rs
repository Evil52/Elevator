use elevator_fw_logic::progress::{ProgressMonitor, Verdict};

#[test]
fn no_progress_holds_the_watchdog_and_names_every_task() {
    let mut monitor = ProgressMonitor::<3>::new();
    assert_eq!(monitor.check(), Verdict::Hold { missing: 0b111 });
}

#[test]
fn progress_of_every_task_feeds_the_watchdog() {
    let mut monitor = ProgressMonitor::<3>::new();
    for task in 0..3 {
        assert!(monitor.progress(task));
    }
    assert_eq!(monitor.check(), Verdict::Feed);
}

#[test]
fn a_feed_starts_a_new_window() {
    let mut monitor = ProgressMonitor::<2>::new();
    monitor.progress(0);
    monitor.progress(1);
    assert_eq!(monitor.check(), Verdict::Feed);
    assert_eq!(monitor.check(), Verdict::Hold { missing: 0b11 });
}

#[test]
fn a_stalled_task_is_named_and_late_progress_is_remembered() {
    let mut monitor = ProgressMonitor::<3>::new();
    monitor.progress(0);
    monitor.progress(2);
    assert_eq!(monitor.check(), Verdict::Hold { missing: 0b010 });
    assert_eq!(monitor.check(), Verdict::Hold { missing: 0b010 });
    monitor.progress(1);
    assert_eq!(monitor.check(), Verdict::Feed);
}

#[test]
fn repeated_progress_of_one_task_does_not_stand_in_for_another() {
    let mut monitor = ProgressMonitor::<2>::new();
    for _ in 0..1_000 {
        monitor.progress(0);
    }
    assert_eq!(monitor.check(), Verdict::Hold { missing: 0b10 });
}

#[test]
fn an_unknown_task_is_rejected_and_counts_for_nothing() {
    let mut monitor = ProgressMonitor::<2>::new();
    assert!(!monitor.progress(2));
    assert!(!monitor.progress(usize::MAX));
    monitor.progress(0);
    assert_eq!(monitor.check(), Verdict::Hold { missing: 0b10 });
}

#[test]
fn a_single_task_monitor_works() {
    let mut monitor = ProgressMonitor::<1>::new();
    assert_eq!(monitor.check(), Verdict::Hold { missing: 1 });
    monitor.progress(0);
    assert_eq!(monitor.check(), Verdict::Feed);
}

#[test]
fn thirty_two_tasks_use_the_whole_mask() {
    let mut monitor = ProgressMonitor::<32>::new();
    assert_eq!(monitor.check(), Verdict::Hold { missing: u32::MAX });
    for task in 0..32 {
        monitor.progress(task);
    }
    assert_eq!(monitor.check(), Verdict::Feed);
    assert!(!monitor.progress(32));
}
