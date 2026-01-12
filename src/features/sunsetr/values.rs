/// Parsed status snapshot from `sunsetr S --json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub active: bool,
    pub next_transition_text: Option<String>,
}

impl Status {
    pub fn inactive() -> Self {
        Self {
            active: false,
            next_transition_text: None,
        }
    }
}
