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

# `waft-plugin-eds` Do not trigger bad calendar calls

```
[2026-02-18T18:17:10Z WARN  waft_eds_daemon] [eds] Refresh failed for /org/gnome/evolution/dataserver/Subprocess/10522/2: org.gtk.GDBus.UnmappedGError.Quark._e_2dclient_2derror_2dquark.Code11: Kalendář nelze obnovit: Nepodporováno
[waft] action abb5c67d-1f12-459e-bc75-45010e9b56cd timed out (app: c7f66ec6-5e64-4be6-970e-eecabae2bfbc)
[2026-02-18T18:17:12Z WARN  waft_eds_daemon] [eds] Refresh failed for /org/gnome/evolution/dataserver/Subprocess/10522/518: org.gtk.GDBus.UnmappedGError.Quark._e_2dclient_2derror_2dquark.Code11: Kalendář nelze obnovit: Nepodporováno
[waft] ActionSuccess for unknown action abb5c67d-1f12-459e-bc75-45010e9b56cd
[2026-02-18T18:25:06Z WARN  waft_eds_daemon] [eds] Refresh failed for /org/gnome/evolution/dataserver/Subprocess/10522/2: org.gtk.GDBus.UnmappedGError.Quark._e_2dclient_2derror_2dquark.Code11: Kalendář nelze obnovit: Nepodporováno
[2026-02-18T18:25:06Z WARN  waft_eds_daemon] [eds] Refresh failed for /org/gnome/evolution/dataserver/Subprocess/10522/518: org.gtk.GDBus.UnmappedGError.Quark._e_2dclient_2derror_2dquark.Code11: Kalendář nelze obnovit: Nepodporováno
```

# `waft-overview` Audio slider does not work

It keeps jumping left and right. It should ignore accepting updates to value from the backend when the slider is being dragged or mousewheeled. When the drag is over, it should accept the value from the outside again.

# `waft-overview` Brightness does not work

The slider always resets the brightness to minimum. It should set the brightness value in realtime as it is sliding instead.

# `waft-settings` categories

Split current settings left panel cards into categories:

- Connectivity (Bluetooth, Wifi, Wired)
- Visual (Display)
- Feedback (Notifications)
- Inputs (Keyboard)

# `waft-settings` Nicer Display UI

Also provide user more options

- Enable/Disable the display via gtk::switch (at least one output must remain active at all times)
- Select resolution and refresh rate as two separated select boxes. Selecting the pixel resolution will limit options provided in refresh rate select
- Select scale (real number rounded to two decimal places)
- Select rotation (0, 90, 180, 270), together with flip it combines into niri tranransform
- Enable/Disable flip (true/false)
- Display readonly connection type and connection ID (HDMI, DisplayPort, Internal, ...), for example DP-3, HDMI-1
- Display readonly physical size

# `waft-settings` Notification sound gallery

Save sounds into waft (`~/.config/waft/sounds`), reference them in the notifications Settings. The point is to be able to sync settings between devices. Have this as a separated page "Sounds" under `Feedback` category.

# `waft-settings` Notifications groups and profiles zero state

These sections need to have a zero state to help users navigate

# `waft-settings` translations

All of the strings in the waft-settings must be translated just as waft-overview. Prepare for a possibility that waft itself will need translations.

# `waft-protocol` entity descriptions

The protocol must communicate human readable descriptions for the entity, the entity properties and the entity methods. The descriptions must be translatable. Each plugin has responsibility for providing the human readable translated descriptions. Each plugin must provide human readable translated Name and Description of itself.

# `waft` usable CLI

Running `waft --help` must provide list of CLI options.

Add global option `[-j|--json]`, that will change the waft CLI output from human readable text to JSON.

The `waft plugin ls` command will provide the human readable Name of the plugin, the plugin id and supported entity list. Text version returns for example "Sunsetr (sunsetr) - night-light, night-light-config". The JSON variant `{ "id": "sunsetr", "name": "Sunsetr", "entities": ["night-light", "night-light-config", "description": "Control sunsetr from waft"] }`

# `waft` CLI protocol

Add command `waft protocol` command will provide list of supported entities and their human readable descriptions.

# `waft` CLI describe plugin

Add command `waft plugin describe [plugin]`, that describes selected plugin.

# `waft-settings` plugins

Add page listing all available waft plugins, their status (available, running, failed, not-running) and their capabilities (entity names). Add this page to new category System.
