//! Sound policy engine for notification sounds.
//!
//! Evaluates whether a notification should trigger a sound, and if so which one.
//! The evaluation follows a priority cascade:
//!
//! 1. Master toggle (disabled -> Silent)
//! 2. DND active (-> Silent)
//! 3. suppress-sound hint (-> Silent)
//! 4. Explicit sound-file hint (-> Play)
//! 5. Explicit sound-name hint (-> Play)
//! 6. Per-app rules (first match wins)
//! 7. Urgency fallback

use crate::config::SoundConfig;
use crate::types::NotificationUrgency;

/// The result of evaluating the sound policy for a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundDecision {
    /// Play the given sound ID (XDG theme name or file path).
    Play(String),
    /// Do not play any sound.
    Silent,
}

/// Context provided to the policy engine for evaluating a single notification.
pub struct NotificationContext<'a> {
    pub app_name: Option<&'a str>,
    pub urgency: NotificationUrgency,
    pub suppress_sound: bool,
    pub sound_file: Option<&'a str>,
    pub sound_name: Option<&'a str>,
    pub category: Option<&'a str>,
    pub dnd_active: bool,
}

/// Immutable sound policy. Loaded once from config, shared via `Arc<SoundPolicy>`.
pub struct SoundPolicy {
    config: SoundConfig,
}

impl SoundPolicy {
    pub fn new(config: SoundConfig) -> Self {
        Self { config }
    }

    /// Evaluate the sound policy for a notification.
    ///
    /// Returns `SoundDecision::Play(sound_id)` or `SoundDecision::Silent`.
    pub fn evaluate(&self, ctx: &NotificationContext<'_>) -> SoundDecision {
        // 1. Master toggle
        if !self.config.enabled {
            return SoundDecision::Silent;
        }

        // 2. DND check
        if ctx.dnd_active {
            return SoundDecision::Silent;
        }

        // 3. suppress-sound hint
        if ctx.suppress_sound {
            return SoundDecision::Silent;
        }

        // 4. Explicit sound-file hint
        if let Some(sound_file) = ctx.sound_file && !sound_file.is_empty() {
            return SoundDecision::Play(sound_file.to_string());
        }

        // 5. Explicit sound-name hint
        if let Some(sound_name) = ctx.sound_name && !sound_name.is_empty() {
            return SoundDecision::Play(sound_name.to_string());
        }

        // 6. Per-app rules (first match wins)
        if let Some(app_name) = ctx.app_name {
            for rule in &self.config.rules {
                if rule.app_name == app_name {
                    // If the rule has a category filter, check it
                    if let Some(ref rule_category) = rule.category {
                        if let Some(notif_category) = ctx.category {
                            if rule_category != notif_category {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    // Empty sound string means silent
                    if rule.sound.is_empty() {
                        return SoundDecision::Silent;
                    }
                    return SoundDecision::Play(rule.sound.clone());
                }
            }
        }

        // 7. Urgency fallback
        let sound = match ctx.urgency {
            NotificationUrgency::Low => &self.config.urgency.low,
            NotificationUrgency::Normal => &self.config.urgency.normal,
            NotificationUrgency::Critical => &self.config.urgency.critical,
        };

        if sound.is_empty() {
            SoundDecision::Silent
        } else {
            SoundDecision::Play(sound.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SoundRule, UrgencySounds};

    fn default_config() -> SoundConfig {
        SoundConfig::default()
    }

    fn default_ctx<'a>() -> NotificationContext<'a> {
        NotificationContext {
            app_name: Some("TestApp"),
            urgency: NotificationUrgency::Normal,
            suppress_sound: false,
            sound_file: None,
            sound_name: None,
            category: None,
            dnd_active: false,
        }
    }

    // Path 1: Master toggle disabled
    #[test]
    fn master_toggle_disabled_returns_silent() {
        let config = SoundConfig {
            enabled: false,
            ..default_config()
        };
        let policy = SoundPolicy::new(config);
        let ctx = default_ctx();
        assert_eq!(policy.evaluate(&ctx), SoundDecision::Silent);
    }

    // Path 2: DND active
    #[test]
    fn dnd_active_returns_silent() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            dnd_active: true,
            ..default_ctx()
        };
        assert_eq!(policy.evaluate(&ctx), SoundDecision::Silent);
    }

    // Path 3: suppress-sound hint
    #[test]
    fn suppress_sound_hint_returns_silent() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            suppress_sound: true,
            ..default_ctx()
        };
        assert_eq!(policy.evaluate(&ctx), SoundDecision::Silent);
    }

    // Path 4: Explicit sound-file hint
    #[test]
    fn sound_file_hint_returns_play() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            sound_file: Some("/usr/share/sounds/custom.ogg"),
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("/usr/share/sounds/custom.ogg".to_string())
        );
    }

    // Path 5: Explicit sound-name hint
    #[test]
    fn sound_name_hint_returns_play() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            sound_name: Some("phone-incoming-call"),
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("phone-incoming-call".to_string())
        );
    }

    // Path 4 takes precedence over path 5
    #[test]
    fn sound_file_takes_precedence_over_sound_name() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            sound_file: Some("/custom/sound.ogg"),
            sound_name: Some("phone-incoming-call"),
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("/custom/sound.ogg".to_string())
        );
    }

    // Path 6: Per-app rule match (silent)
    #[test]
    fn per_app_rule_silent_returns_silent() {
        let config = SoundConfig {
            rules: vec![SoundRule {
                app_name: "Spotify".to_string(),
                sound: String::new(),
                category: None,
            }],
            ..default_config()
        };
        let policy = SoundPolicy::new(config);
        let ctx = NotificationContext {
            app_name: Some("Spotify"),
            ..default_ctx()
        };
        assert_eq!(policy.evaluate(&ctx), SoundDecision::Silent);
    }

    // Path 6: Per-app rule match (custom sound)
    #[test]
    fn per_app_rule_custom_sound_returns_play() {
        let config = SoundConfig {
            rules: vec![SoundRule {
                app_name: "Signal".to_string(),
                sound: "phone-incoming-call".to_string(),
                category: None,
            }],
            ..default_config()
        };
        let policy = SoundPolicy::new(config);
        let ctx = NotificationContext {
            app_name: Some("Signal"),
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("phone-incoming-call".to_string())
        );
    }

    // Path 6: Per-app rule with category filter
    #[test]
    fn per_app_rule_with_matching_category() {
        let config = SoundConfig {
            rules: vec![SoundRule {
                app_name: "Firefox".to_string(),
                sound: "message-new-instant".to_string(),
                category: Some("im.received".to_string()),
            }],
            ..default_config()
        };
        let policy = SoundPolicy::new(config);

        // Matching category
        let ctx = NotificationContext {
            app_name: Some("Firefox"),
            category: Some("im.received"),
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("message-new-instant".to_string())
        );

        // Non-matching category falls through to urgency
        let ctx = NotificationContext {
            app_name: Some("Firefox"),
            category: Some("email.arrived"),
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("message-new-email".to_string())
        );
    }

    // Path 6: Per-app rule with category, notification has no category
    #[test]
    fn per_app_rule_with_category_no_notif_category_skips() {
        let config = SoundConfig {
            rules: vec![SoundRule {
                app_name: "Firefox".to_string(),
                sound: "message-new-instant".to_string(),
                category: Some("im.received".to_string()),
            }],
            ..default_config()
        };
        let policy = SoundPolicy::new(config);

        // No category on notification -> rule doesn't match, falls through to urgency
        let ctx = NotificationContext {
            app_name: Some("Firefox"),
            category: None,
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("message-new-email".to_string())
        );
    }

    // Path 7: Urgency fallback
    #[test]
    fn urgency_fallback_low() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            urgency: NotificationUrgency::Low,
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("message-new-instant".to_string())
        );
    }

    #[test]
    fn urgency_fallback_normal() {
        let policy = SoundPolicy::new(default_config());
        let ctx = default_ctx();
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("message-new-email".to_string())
        );
    }

    #[test]
    fn urgency_fallback_critical() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            urgency: NotificationUrgency::Critical,
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("dialog-warning".to_string())
        );
    }

    // Edge case: empty urgency sound
    #[test]
    fn empty_urgency_sound_returns_silent() {
        let config = SoundConfig {
            urgency: UrgencySounds {
                low: String::new(),
                normal: String::new(),
                critical: String::new(),
            },
            ..default_config()
        };
        let policy = SoundPolicy::new(config);
        let ctx = default_ctx();
        assert_eq!(policy.evaluate(&ctx), SoundDecision::Silent);
    }

    // Edge case: no app name
    #[test]
    fn no_app_name_skips_rules_falls_to_urgency() {
        let config = SoundConfig {
            rules: vec![SoundRule {
                app_name: "Spotify".to_string(),
                sound: String::new(),
                category: None,
            }],
            ..default_config()
        };
        let policy = SoundPolicy::new(config);
        let ctx = NotificationContext {
            app_name: None,
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("message-new-email".to_string())
        );
    }

    // Edge case: first rule match wins
    #[test]
    fn first_matching_rule_wins() {
        let config = SoundConfig {
            rules: vec![
                SoundRule {
                    app_name: "Signal".to_string(),
                    sound: "first-sound".to_string(),
                    category: None,
                },
                SoundRule {
                    app_name: "Signal".to_string(),
                    sound: "second-sound".to_string(),
                    category: None,
                },
            ],
            ..default_config()
        };
        let policy = SoundPolicy::new(config);
        let ctx = NotificationContext {
            app_name: Some("Signal"),
            ..default_ctx()
        };
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("first-sound".to_string())
        );
    }

    // Edge case: empty sound-file hint is ignored
    #[test]
    fn empty_sound_file_hint_ignored() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            sound_file: Some(""),
            ..default_ctx()
        };
        // Should fall through to urgency
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("message-new-email".to_string())
        );
    }

    // Edge case: empty sound-name hint is ignored
    #[test]
    fn empty_sound_name_hint_ignored() {
        let policy = SoundPolicy::new(default_config());
        let ctx = NotificationContext {
            sound_name: Some(""),
            ..default_ctx()
        };
        // Should fall through to urgency
        assert_eq!(
            policy.evaluate(&ctx),
            SoundDecision::Play("message-new-email".to_string())
        );
    }
}
