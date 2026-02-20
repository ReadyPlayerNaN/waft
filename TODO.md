# WiFi: Support connecting to new (unsaved) networks

Currently WiFi only shows networks with saved connection profiles. Connecting to new networks requires a password prompt flow using `AddAndActivateConnection()` on the NetworkManager D-Bus Settings interface.

# Notification toast bubbles

Think of having the toasts as unobtrusive bubbles

# Meeting invite

Whenever an email with meeting invite is received, present it as calendar notification. Provide actions to accept, decline, maybe and open calendar.

# SNI tray

Consider having the SNI tray in the waft overview.
(Status Notifier Items)

# Launcher

Create app `waft-launcher`. The app will have a single window on the gtk layer shell. On start it will focus an input field. Upon search, the app will search available applications. It will have keyboard navigation - using arrows up and down will change the selected item, but the input field remains focused. `<Enter>` starts the selected app and exits `waft-launcher`. Clicking an app starts it and exits `waft-launcher`. The list of apps will display icon and name.

# `waft-overview` Audio slider even now still does not work

This is the fifth iteration. The audio slider lags, making it unusable

- The audio sliders now move when dragging. That is good.
- The slider was sends the value to the backend during the drag immediately.
- The slider value is being overriden by backend value during the drag, that is bad.

It looks like we have regressed and now the slider is unusable. There should be two modes: idle and dragging.

## Idle slider

When it is idle (not dragging), the slider immediately reactively displays the value that comes from the backend (for example audio plugin). Every new value.

## Dragging slider

When it is dragging (not idle), the slider respects the value provided by the user and completely ignores value provided by the backend. The dragging starts with user mousedown event on the drag ball and ends with user releasing the dragball.

---

# `waft-settings` Wallpaper modes

Wallpaper manager will have three modes to work with:

## Static mode

Static mode is simple: Select a wallpaper, the wallpaper gets stored in the main wallpaper folder and you are done.

## Style tracking mode

There is a folder `dark` and `light` in the configured wallpaper folder. Whenever a `dark-mode` entity changes, the wallpaper rotates to match the mode. When no matching wallpaper is found, it is a noop. The wallpaper plugin listens for the `dark-mode` entity. This mode is unavailable when nothing provides the `dark-mode` entitty

## Day tracking mode

There are folders matching parts of the day. Whenever current day progresses to the next part, wallpaper is rotated and a random one is picked from one of following directories localed in the configured wallpaper directory. When no matching wallpaper is found, it is a noop.

- `early-morning` approx. 4:30 - 7:30
- `morning` approx 7:30 – 12:00
- `afternoon` 12:00 - 17:00
- `evening` 17:00 - 21:00
- `night` 21:00 - 1:00
- `midnight-oil` - 1:00 - 4:30

---

# `waft-plugin-audio` device type

Propagate audio card type through the protocol. The `AudioCard` will provide `bus = device.bus` and `device_type = device.form_factor` of the pulse audio device. AudioDevice will also provide the `device_type`. This should replace the `AudioDevice.icon` and `AudioDevice.connection_icon`. The icon decisions will be done in the `waft-ui-gtk`, we will have `AudioDeviceIcon` and `AudioConnectionIcon` components, that resolve the icon based on `direction=input/output`, `connection_type`, `device_type`.

## Example: Webcam

Card has `device.form_factor = webcam` -> device type is `webcam` AND `device.bus = usb`.
`AudioDevice` will provide:

- `connection_type = usb`.
- `device_type = webcam`.

## Example: Bluetooth headset

Card has `form_factor = headset` -> device type is `headset` AND Card has `device.bus = bluetooth`

- `connection_type = bluetooth`.
- `device_type = headset`.

## Example: PCI Card

Card has `form_factor = NULL` AND Card has `device.bus = pci`.

Both the `AudioCard` and `AudioDevice` entities provide:

- `device_type = card`.

The `AudioDevice` entities are derived from port type

Port type = Line -> `connection_type = jack`.
Port type = Headphones -> `connection_type = jack`.
