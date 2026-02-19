# WiFi: Support connecting to new (unsaved) networks

Currently WiFi only shows networks with saved connection profiles. Connecting to new networks requires a password prompt flow using `AddAndActivateConnection()` on the NetworkManager D-Bus Settings interface.

# Notification toast bubbles

Think of having the toasts as unobtrusive bubbles

# Meeting invite

Whenever an email with meeting invite is received, present it as calendar notification. Provide actions to accept, decline, maybe and open calendar.

# SNI tray

Consider having the SNI tray in the waft overview.
(Status Notifier Items)

# `waft-settings`

Audio settings that include managing input / output devices and their settings as `pavucontrol` does.

# Launcher

Create app `waft-launcher`. The app will have a single window on the gtk layer shell. On start it will focus an input field. Upon search, the app will search available applications. It will have keyboard navigation - using arrows up and down will change the selected item, but the input field remains focused. `<Enter>` starts the selected app and exits `waft-launcher`. Clicking an app starts it and exits `waft-launcher`. The list of apps will display icon and name.

---

# `waft-overview` Bluetooth paired label

When no devices are connected to a bluetooth adatper, it displays label "{n} paired". There should be no label when all devices are disconnected.

# `waft-settings` Keyboard layout options

Must be able to configure keyboard layout options. For example cz QWERTY.

# `waft-settings` Bluetooth search button

Should be in the header of "available devices" on the right side.

# `waft-settings` More granular search

Currently search finds and focuses more-or-less sections. It is good. But we need to do better. We should make every field in the `waft-settings`, that is provided by the UI (this includes disabled fields) also findable. When it is findable, it must be focusable.

# `waft-settings` Wallpaper settings

Create `waft-plugin-swww` that allows managing wallpapers using `swww`. Take the script `~/.config/niri/wallpaper-rotate.sh` as a base of logic. The plugin should provide `waft-settings` methods to change the wallpaper, set the random wallpaper, configure the wallpaper directory, configure transition fps, angle, duration and type. I think a good name for such entity is `WallpaperManager`. It should provide an option to select wallpaper per monitor or to keep it in sync. Default wallpaper folder is `~/.config/waft/wallpapers`, but it can be changed. Put this under visual section of settings

# `waft-overview` Audio slider even now still does not work

The audio sliders now move when dragging. That is good. The slider is not being overriden by backend value during the drag, that is also good. The slider was supposed to send the value to the backend during the drag immediately.

# App integration

Introduce entity `WaftSettings`. This entity is going to be provided by a new `waft-plugin-internal-apps`. The entity will be provided only if the binary `waft-settings` is available and executable in the `$PATH`. It will provide two methods:

- `open` - Opens waft settings as usual
- `openPage` - Opens waft settings on a specific page

Each waft settings page must have static identificator assigned.

## `waft-overview` settings integration

There should be a general settings button in the header, right of the keyboard layout button. It should be visible only if the `WaftSettings` entity is available.

### Wired settings button

The wired feature toggle menu should have "Settings" button that opens waft-settings on the wired page using the WaftSettings entity method openPage. The menu will be available even if wired connection is disconnected.

### Bluetooth settings button

The bluetooth feature toggle menu should have "Settings" button that opens waft-settings on the bluetooth page using the WaftSettings entity method openPage. The menu will be available even if bluetooth adapter is off.

# Network manager plugin: wireguard

The VPN list now correctly shows the wireguard VPN connections. However, connecting it does not seem to work. This may be because of the fact that wireguard connections are triggered differently to wireguard. The Vpn entity in the protocol may have to be extended to also include "type": "vpn | wireguard".

# `waft-overview` VPN feature toggle menu icons

- Regular VPN must display VPN icon
- Wireguard networks must display wireguard icon

# `waft` logging too verbose

Revise the logged messages in waft to see if all of them are really needed to be at `info` level. What is not helpful on info may be moved to `debug` or `trace` level.
