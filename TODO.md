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

# Launcher entry animation

Make the launcher fade in and fade out just like the overview does

# Launcher too slow response

It takes to long when I enter a character into the launcher input to render the suggested app list. This needs to be absolutely instant.

# Plugin integration tests

All plugins (especially niri) need to have integration tests.

# `waft-settings` / Wallpaper highlight

When I navigate to the View / Wallpaper, the active wallpaper must be selected in the gallery. Currently none is selected, despite the current file is on the desktop

# `waft-settings` / Virtual audio devices

Have the ability to create and persist virtual audio devices using pipewire / pulse

# `waft-settings` / Scroll plane scrolls without content

When I open any page, it scrolls down, like it would be at least 2000 px tall. However, the content is usually small, so scrolling down only hides the content. The page scroll area must be as tall as the page content

# `waft-settings` / View / Appearance

The dark mode and the night light settings each use different way to display the settings. Please unify it, so there is only one way. By the way, the dark mode settings do not work at all, it does nothing on click. So visually, the night light looks better, however it is still cringy, because it renders two close buttons on the right top, that should not be
