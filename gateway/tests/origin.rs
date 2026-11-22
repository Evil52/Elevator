use elevator_gateway::OriginPolicy;

fn policy(missing: bool) -> OriginPolicy {
    OriginPolicy::new(
        vec![
            "http://localhost:5173".to_owned(),
            "https://panel.example".to_owned(),
        ],
        missing,
    )
}

#[test]
fn listed_origins_are_allowed() {
    let policy = policy(false);
    assert!(policy.allows(Some("http://localhost:5173")));
    assert!(policy.allows(Some("https://panel.example")));
}

#[test]
fn other_origins_are_refused_even_when_they_look_similar() {
    let policy = policy(true);
    for origin in [
        "http://localhost:5174",
        "http://localhost:5173/",
        "https://panel.example.evil.com",
        "HTTP://LOCALHOST:5173",
        "null",
        "",
    ] {
        assert!(!policy.allows(Some(origin)), "{origin}");
    }
}

#[test]
fn a_missing_origin_follows_the_setting() {
    assert!(policy(true).allows(None));
    assert!(!policy(false).allows(None));
}

#[test]
fn an_empty_list_allows_nothing_with_an_origin() {
    let policy = OriginPolicy::new(Vec::new(), false);
    assert!(!policy.allows(Some("http://localhost:5173")));
    assert!(!policy.allows(None));
}
