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

---

# `waft-toasts` timeout dismisses notifications

It seems that notifications that time out in `waft-toasts` also trigger dismiss. This should not be. The `waft-toasts`. The notification arrives to `waft-toasts` either with ttl or without ttl.

## With TTL

This is the timeout given by the sender of the notification and it is binding. When we receive notification with TTL, we must honor the TTL and let the notification disappear on its own and mark it as timed out. These must be given higher priority than the notifications without TTL (while also considering urgency).

Normal urgency with TTL > Normal urgency without TTL
Urgent with TTL > Urgent without TTL

## Without TTL

Notifications that arrive without TTL are supposed to behave under `waft-toasts` terms. The app stores them in memory. When they are displayed, they should be visible as toasts only for a limited time. When they time out, the toasts disappear, but the notification is supposed to remain accessible from other apps. The only acceptable scenario when a toast times out and it is removed is that no other app wants it.

## No other app wants it

There needs to be a special Notification entity method to confirm this. Something like "release". The query is sent to the `waft-plugin-notifications`. The plugin daemon sends a message via a new entity `EntityClaim(entity="Notification", ident={notification_id})` carrying the notification ID. When `waft-overview` receives it and it has the notification in its store, it responds with using `EntityClaim.claim`. If it does not have it in memory, it responds with the method `EntityClaim.pass`. This is what we would do to avoid memory leaks (`waft-toasts` does not know if it can afford to release it without loosing other apps data.


