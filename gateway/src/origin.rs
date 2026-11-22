#[derive(Clone, Debug)]
pub struct OriginPolicy {
    allowed: Vec<String>,
    allow_missing: bool,
}

impl OriginPolicy {
    #[must_use]
    pub const fn new(allowed: Vec<String>, allow_missing: bool) -> Self {
        Self {
            allowed,
            allow_missing,
        }
    }

    #[must_use]
    pub fn allows(&self, origin: Option<&str>) -> bool {
        origin.map_or(self.allow_missing, |origin| {
            self.allowed.iter().any(|allowed| allowed == origin)
        })
    }
}
