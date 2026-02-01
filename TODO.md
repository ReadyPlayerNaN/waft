## Plugins to implement
- WiFi plugin (NetworkManager module is partially built)
- Caffeine plugin (not sure how)
- Tether plugin?
- SNI

## Architecture
- Create strategy for "Failed to load widgets"

## Feature completeness
- Notifications: wire up toast window present/hide methods
- Notifications: implement notification group get_notification, get_panel_notifications methods
- Notifications: implement NotificationDisplay struct for UI
- Brightness: connect store subscription to widget updates (set_displays, update_brightness)
- Keyboard layout: implement switch_prev action in UI
