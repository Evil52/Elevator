use elevator_ctrl::motion::MotionParams;

#[test]
fn samples_the_same_jerk_limited_trip_as_eta() {
    let params = MotionParams::new(3000, 1500, 3000).unwrap();
    let trip = params.rest_trip(9000).unwrap();
    assert_eq!(Some(trip.duration_ms()), params.time_to_stop_ms(9000, 0));
    assert_eq!(trip.duration_ms(), 5500);
    for (t, position, speed) in [
        (0, 0, 0),
        (250, 8, 94),
        (500, 63, 375),
        (1250, 766, 1500),
        (2000, 2313, 2625),
        (2500, 3750, 3000),
        (2750, 4500, 3000),
        (3000, 5250, 3000),
        (4250, 8234, 1500),
        (5000, 8938, 375),
        (5500, 9000, 0),
        (6000, 9000, 0),
    ] {
        assert_eq!(trip.sample(t), (position, speed), "t={t}");
    }
}

#[test]
fn short_long_and_stationary_trips_keep_the_eta_and_endpoints() {
    for params in [
        MotionParams::new(3000, 1500, 3000).unwrap(),
        MotionParams::new(100, 1, 7).unwrap(),
        MotionParams::new(u32::MAX, u32::MAX, u32::MAX).unwrap(),
    ] {
        for distance in [0, 1, 17, 4321, 9000, 27000, u32::MAX] {
            let eta = params.time_to_stop_ms(distance, 0).unwrap();
            let Some(trip) = params.rest_trip(distance) else {
                assert_eq!(eta, u32::MAX);
                continue;
            };
            assert_eq!(trip.duration_ms(), eta);
            let mut previous = 0;
            for i in 0..=100u32 {
                let t = u32::try_from(u64::from(eta) * u64::from(i) / 100).unwrap();
                let (mm, speed) = trip.sample(t);
                assert!((previous..=distance).contains(&mm));
                assert!(speed <= params.max_speed_mm_s());
                previous = mm;
            }
            assert_eq!(trip.sample(0), (0, 0));
            assert_eq!(trip.sample(eta), (distance, 0));
        }
    }
}
