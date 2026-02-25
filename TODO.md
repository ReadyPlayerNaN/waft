# WiFi: Support connecting to new (unsaved) networks

Currently WiFi only shows networks with saved connection profiles. Connecting to new networks requires a password prompt flow using `AddAndActivateConnection()` on the NetworkManager D-Bus Settings interface.

# Notification toast bubbles

Think of having the toasts as unobtrusive bubbles

# Meeting invite

Whenever an email with meeting invite is received, present it as calendar notification. Provide actions to accept, decline, maybe and open calendar.

# SNI tray

Consider having the SNI tray in the waft overview.
(Status Notifier Items)

---

# `waft-settings` View / Appearance Collapsibles

The page is crowded and most of the options are dark mode or night light specific and they are not something the user would reach for quite often.

Please create another page "Dark Mode Settings" (do not list it in the left menu). Add a link to this under Dark Mode switch item.

Please create another page "Night light Settings" (do not list it in the left menu). Add a link to this under Night light next transition item.

# `waft-settings` View / Appearance GTK accent colour

Read and configure GTK accent colour from the current theme. Add this to settings under the Night Light settings

# `waft-settings` Niri appearance settings

These settings will modify niri config. Please make sure we do the same serialization as in other sections. Keeping the niri config unbroken is CRITICAL.

## Add to View/Wallpaper

- background color settings

## Add to View/Windows

Include all these with all their options

- focus-ring
- border
- shadow
- tab indicator
- gaps
- struts

### Deriving colours

Add a special switch "Derive colours from GTK Theme". Switching this will disable all colour selection for windows and all the colours will be derived from the GTK accent colour

# `waft-toasts` default action

Left clicking a toast should trigger the default action (same as notifications widget in waft-overlay).
