# Notification sound gallery

Save sounds into waft, reference them in the notifications Settings

# Notifications groups and profiles zero state

These sections need to have a zero state to help users navigate

# WiFi: Support connecting to new (unsaved) networks

Currently WiFi only shows networks with saved connection profiles. Connecting to new networks requires a password prompt flow using `AddAndActivateConnection()` on the NetworkManager D-Bus Settings interface.

# Notification toast bubbles

Think of having the toasts as unobtrusive bubbles

# Meeting invite

Whenever an email with meeting invite is received, present it as calendar notification. Provide actions to accept, decline, maybe and open calendar.

# SNI tray

Consider having the SNI tray in the waft overview.
(Status Notifier Items)

# Do not trigger bad calendar calls

```
[2026-02-18T18:17:10Z WARN  waft_eds_daemon] [eds] Refresh failed for /org/gnome/evolution/dataserver/Subprocess/10522/2: org.gtk.GDBus.UnmappedGError.Quark._e_2dclient_2derror_2dquark.Code11: Kalendář nelze obnovit: Nepodporováno
[waft] action abb5c67d-1f12-459e-bc75-45010e9b56cd timed out (app: c7f66ec6-5e64-4be6-970e-eecabae2bfbc)
[2026-02-18T18:17:12Z WARN  waft_eds_daemon] [eds] Refresh failed for /org/gnome/evolution/dataserver/Subprocess/10522/518: org.gtk.GDBus.UnmappedGError.Quark._e_2dclient_2derror_2dquark.Code11: Kalendář nelze obnovit: Nepodporováno
[waft] ActionSuccess for unknown action abb5c67d-1f12-459e-bc75-45010e9b56cd
[2026-02-18T18:25:06Z WARN  waft_eds_daemon] [eds] Refresh failed for /org/gnome/evolution/dataserver/Subprocess/10522/2: org.gtk.GDBus.UnmappedGError.Quark._e_2dclient_2derror_2dquark.Code11: Kalendář nelze obnovit: Nepodporováno
[2026-02-18T18:25:06Z WARN  waft_eds_daemon] [eds] Refresh failed for /org/gnome/evolution/dataserver/Subprocess/10522/518: org.gtk.GDBus.UnmappedGError.Quark._e_2dclient_2derror_2dquark.Code11: Kalendář nelze obnovit: Nepodporováno
```

# Brightness does not work

The slider always resets the brightness to minimum

# Audio slider does not work

It keeps jumping left and right. It should avoid accepting updates from the backend when the slider is being dragged.
