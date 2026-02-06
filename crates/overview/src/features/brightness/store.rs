//! Brightness state management.

use std::cell::RefCell;

/// Type of display for icon selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayType {
    /// Laptop/internal backlight (via brightnessctl)
    Backlight,
    /// External monitor (via ddcutil DDC/CI)
    External,
}

/// Information about a controllable display.
#[derive(Debug, Clone)]
pub struct Display {
    pub id: String,
    pub name: String,
    pub display_type: DisplayType,
    pub brightness: f64,
}

/// Brightness plugin state.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct BrightnessState {
    pub available: bool,
    pub displays: Vec<Display>,
}


/// Operations for updating brightness state.
#[derive(Debug, Clone)]
pub enum BrightnessOp {
    Available(bool),
    Displays(Vec<Display>),
    Brightness { display_id: String, brightness: f64 },
}

/// Brightness state store.
pub struct BrightnessStore {
    state: RefCell<BrightnessState>,
    subscribers: RefCell<Vec<Box<dyn Fn()>>>,
}

impl BrightnessStore {
    fn new() -> Self {
        Self {
            state: RefCell::new(BrightnessState::default()),
            subscribers: RefCell::new(Vec::new()),
        }
    }

    /// Get the current state.
    pub fn get_state(&self) -> BrightnessState {
        self.state.borrow().clone()
    }

    /// Emit an operation to update state.
    pub fn emit(&self, op: BrightnessOp) {
        {
            let mut state = self.state.borrow_mut();
            match op {
                BrightnessOp::Available(available) => {
                    state.available = available;
                }
                BrightnessOp::Displays(displays) => {
                    state.displays = displays;
                }
                BrightnessOp::Brightness {
                    display_id,
                    brightness,
                } => {
                    if let Some(display) = state.displays.iter_mut().find(|d| d.id == display_id) {
                        display.brightness = brightness;
                    }
                }
            }
        }

        // Notify subscribers
        for subscriber in self.subscribers.borrow().iter() {
            subscriber();
        }
    }

    /// Subscribe to state changes.
    pub fn subscribe<F: Fn() + 'static>(&self, callback: F) {
        self.subscribers.borrow_mut().push(Box::new(callback));
    }
}

/// Create a new brightness store.
pub fn create_brightness_store() -> BrightnessStore {
    BrightnessStore::new()
}

/// Compute the master brightness value (average of all displays).
pub fn compute_master_average(displays: &[Display]) -> f64 {
    if displays.is_empty() {
        return 0.0;
    }

    let sum: f64 = displays.iter().map(|d| d.brightness).sum();
    sum / displays.len() as f64
}

/// Apply proportional scaling to all displays based on master slider change.
///
/// Returns a vector of (display_id, new_brightness) tuples.
pub fn compute_proportional_scaling(
    displays: &[Display],
    old_master: f64,
    new_master: f64,
) -> Vec<(String, f64)> {
    if displays.is_empty() {
        return Vec::new();
    }

    // Special case: when old_master is 0 (or very close), use additive scaling
    if old_master < 0.001 {
        return displays
            .iter()
            .map(|d| (d.id.clone(), new_master))
            .collect();
    }

    // Normal proportional scaling: new_value = current_value * (new_master / old_master)
    let ratio = new_master / old_master;
    displays
        .iter()
        .map(|d| {
            let new_brightness = (d.brightness * ratio).clamp(0.0, 1.0);
            (d.id.clone(), new_brightness)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_master_average_empty() {
        assert_eq!(compute_master_average(&[]), 0.0);
    }

    #[test]
    fn test_compute_master_average_single() {
        let displays = vec![Display {
            id: "test".to_string(),
            name: "Test".to_string(),
            display_type: DisplayType::Backlight,
            brightness: 0.75,
        }];
        assert!((compute_master_average(&displays) - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_compute_master_average_multiple() {
        let displays = vec![
            Display {
                id: "a".to_string(),
                name: "A".to_string(),
                display_type: DisplayType::Backlight,
                brightness: 0.5,
            },
            Display {
                id: "b".to_string(),
                name: "B".to_string(),
                display_type: DisplayType::External,
                brightness: 0.9,
            },
        ];
        assert!((compute_master_average(&displays) - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_proportional_scaling_up() {
        let displays = vec![
            Display {
                id: "a".to_string(),
                name: "A".to_string(),
                display_type: DisplayType::Backlight,
                brightness: 0.25,
            },
            Display {
                id: "b".to_string(),
                name: "B".to_string(),
                display_type: DisplayType::External,
                brightness: 0.45,
            },
        ];
        // Master from 0.35 to 0.70 (2x)
        let result = compute_proportional_scaling(&displays, 0.35, 0.70);
        assert_eq!(result.len(), 2);
        assert!((result[0].1 - 0.5).abs() < 0.001); // 0.25 * 2 = 0.5
        assert!((result[1].1 - 0.9).abs() < 0.001); // 0.45 * 2 = 0.9
    }

    #[test]
    fn test_proportional_scaling_to_zero() {
        let displays = vec![Display {
            id: "a".to_string(),
            name: "A".to_string(),
            display_type: DisplayType::Backlight,
            brightness: 0.5,
        }];
        let result = compute_proportional_scaling(&displays, 0.5, 0.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1, 0.0);
    }

    #[test]
    fn test_proportional_scaling_from_zero() {
        let displays = vec![
            Display {
                id: "a".to_string(),
                name: "A".to_string(),
                display_type: DisplayType::Backlight,
                brightness: 0.0,
            },
            Display {
                id: "b".to_string(),
                name: "B".to_string(),
                display_type: DisplayType::External,
                brightness: 0.0,
            },
        ];
        // When at 0, use additive: all displays get the new master value
        let result = compute_proportional_scaling(&displays, 0.0, 0.5);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1, 0.5);
        assert_eq!(result[1].1, 0.5);
    }

    #[test]
    fn test_proportional_scaling_clamps() {
        let displays = vec![Display {
            id: "a".to_string(),
            name: "A".to_string(),
            display_type: DisplayType::Backlight,
            brightness: 0.8,
        }];
        // Try to scale beyond 1.0
        let result = compute_proportional_scaling(&displays, 0.4, 0.8);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1, 1.0); // Clamped to 1.0
    }
}
