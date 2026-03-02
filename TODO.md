# Notification toast bubbles

Think of having the toasts as unobtrusive bubbles

# Meeting invite

Whenever an email with meeting invite is received, present it as calendar notification. Provide actions to accept, decline, maybe and open calendar.

# SNI tray

Consider having the SNI tray in the waft overview.
(Status Notifier Items)

---

# `waft-launcher` rows

When the row item text is too long, break it. Row item stretching two rows is more acceptable than horizontaly stretching the launcher window. Enlarge the icon to 24px.

Prefix window rows labels with W and app row labels with A.

The light theme is unreadable, because there are some fixed colours. The font of row item is white.

# `waft-launcher` close on unfocus

The launcher window needs to disappear when it looses focus. The same way as the overview window

# `waft-settings` wallpapers

Each of the galleries must accept drag and drop files from other apps, such as nautilus drag and dropping. It must be possible to drag and drop wallpaper from one gallery, moving it to another gallery on this page. Double clicking wallpaper must open the image in default viewer (probably best way is to use xdg-open)

# `waft-settings` audio devices

Please group each audio device stuff into some kind of a visual container.

Add the ability to create virtual audio devices. It looks like there is already a list (empty list), but no UI way to create a virtual audio device.

# `waft-settings` tweaks

The page, even when empty, can scroll thousands of pixels. This makes no sense. The scroll must match the content height of the active page.

Allow changing layout from the `waft-settings` layouts view by adding radio button to each of the layouts. Display visualisation of keyboard layout under the selection of layouts.

The shortcuts page should be more consistent - the label on a reserved space, the shortcut on a reserved space, the action type (for example spawn) on a reserverd place. The edit and delete button should be icon buttons only. Group shortcuts by action type, visually split the groups.

Bluetooth devices list icon size - reduce to 24px.

# `waft-settings` Online Accounts

Please add page "Online accounts" into `waft-settings`. It will list the active online accounts provided by entity `OnlineAccount`, provided by the new `waft-plugin-gnome`. It will only list accounts, show account status, allow toggle services. No adding accounts and no changing of the account config even when the status is actionNeeded or credentialsneedattention.
