/// Parsed status snapshot from `sunsetr S --json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// True if sunsetr process is running, false if not
    pub active: bool,
    /// Current period: "day" or "night" (or other custom periods)
    pub period: Option<String>,
    /// Time of next transition (HH:MM format)
    pub next_transition_text: Option<String>,
}

impl Status {
    pub fn inactive() -> Self {
        Self {
            active: false,
            period: None,
            next_transition_text: None,
        }
    }

    pub fn is_night_period(&self) -> bool {
        self.period
            .as_ref()
            .map(|p| !p.eq_ignore_ascii_case("day"))
            .unwrap_or(false)
    }
}
