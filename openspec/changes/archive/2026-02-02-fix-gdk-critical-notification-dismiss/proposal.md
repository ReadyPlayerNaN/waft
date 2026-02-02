## Why

The notifications plugin emits GTK/Gdk CRITICAL errors when dismissing notifications from the widget. This occurs because widget removal happens during active event processing, causing GTK to access destroyed widgets and surfaces. The errors degrade user experience and indicate unsafe memory access patterns that could lead to undefined behavior.

## What Changes

- Fix race condition between revealer animation completion and widget removal
- Ensure gesture handlers check widget validity before accessing widget properties
- Centralize widget lifecycle management to prevent double-removal
- Separate animation completion from actual widget destruction

## Capabilities

### New Capabilities

- `safe-widget-removal`: Patterns for safely removing widgets during animation callbacks without triggering GTK assertions

### Modified Capabilities

- `notifications`: Add requirements for safe widget lifecycle management during notification dismissal

## Impact

- **Code affected**: `notification_card.rs`, `toast_widget.rs`, `notification_group.rs`
- **Key changes**:
  - Revealer `connect_child_revealed_notify` callbacks in `notification_card.rs:77-88` and `toast_widget.rs:86-95`
  - Gesture handlers in `notification_card.rs:118-130` and `toast_widget.rs:133-150`
  - Card removal logic in `notification_group.rs:254-258`
- **No API changes**: This is an internal fix; external interfaces remain unchanged
- **Dependencies**: None added; uses existing GTK4/Relm4 APIs
