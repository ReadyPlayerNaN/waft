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

# Revisit coding skills

We would like to push usage of `RenderFn`. Please improve the `dumb-widget-smart-container`, so it matches current preferred coding standards for this project. Maybe rename the skill to `widget-coding`, so it is more discoverable.

# `waft-settings` user services

No services are showing up in waft settings / services

# `waft-plugin-notifications` recording

The recording switch is not present in the waft-settings / notifications

# `waft-overview` right column tabs

The right column in the default XML layout is statically defined. I would like it to be tabbable column. The tab pills should be detached from the tab panes. If there is a GTK component we can use, use it

## Content view

The left column must stay as-is. The right column content will be wrapped in the Tab view. The current content is going to be in a tab view named "controls", visible by default. Add another view (hidden by default), that is going to contain the session actions (Lock Screen, Logout, Reboot, Suspend, Power off). Only one tab view can be active at a time.

## Header view

The right side of header should contain following buttons

```
Keyboard layout | Settings | Controls (Tab switch) | Exit (Tab switch)
```

The Controls Tab button switches the tab to controls, the exit button switches the right column tab to exit options.

# Audio device name duplicates

User may plug in multiple devices, that have the same name, like for example `HyperX QuadCast S`. We need to spice up the name, so the user can distinguish between them.

# Calendar widget week number

Display week numbers left of the dates in the calendar widget

# `waft-settings` niri startup

Create page System / Startup, allow adding, editting and removing `spawn-at-startup` items. One item per row. The editor must support:

- adding arguments to the command
- forward compatibility - when the config is invalid, the input editor MUST NOT break the config and MUST AT ALL COSTS prevent user loosing his config

# `waft-settings` niri binds

Create page Inputs / Keyboard shortcuts and allow adding, editting and removing `binds`. The shortcut editor must support:

- input validation - the key names must be valid
- `hotkey-overlay-title` - the user may optionally enter the title
- `allow-when-locked` - checkbox that allows triggering the shortcut when sesion is locked
- provide options (spawn + command, or niri action from enum)
- forward compatibility - when the config is invalid, the input editor MUST NOT break the config and MUST AT ALL COSTS prevent user loosing his config

# `waft-settings` wallpaper mode buttons

The page View / Wallpaper has the wallpaper selection button across the wallpaper modes. Instead, th "Add Wallpaper" or "Browse" button should disappear when user selects mode different thant "Static".

# `waft-settings` wallpaper gallery

There should be a gallery of square wallpaper previews (cover style). Static gallery should display all of them. Dark mode gallery should have one gallery for dark mode and one for light mode. Day mode gallery should have one gallery for each time of day. The galleries should be under all of the page's current content. Only the galleries for the selected mode should be visible. The selected wallpaper is highlighted. Clicking a wallpaper in the gallery immediately changes the selected wallpaper. Each gallery must have a title and there must be an add button right of the gallery title
