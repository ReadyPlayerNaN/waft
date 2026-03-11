# Notification toast bubbles

Think of having the toasts as unobtrusive bubbles

# Meeting invite

Whenever an email with meeting invite is received, present it as calendar notification. Provide actions to accept, decline, maybe and open calendar.

# SNI tray

Consider having the SNI tray in the waft overview.
(Status Notifier Items)

---

# `waft-overview` Maximum window height

The overview window MUST NOT exceed the viewport height minus the margins. If the content pushes the window to grow too much, it must be scrollable. Please make the window header fixed.

# `waft-launcher` locales

The waft launcher displays app names in original/English. It should display translated app names whenever available.

# `waft-launcher` response too slow

Investigate why is the response time so slow and fix it. It should be at least as fast as wofi/rofi.

## Scenario

Type short string into search input, for example "vol".

The result: The launcher renders results for "v" by the time "vol" has been typed, then renders resutls for "vo" and then renders results for "vol", which looks awkward and unstable.

Expected result: Before the user manages to type in "o", the view should rerender and provide results. It must be faster than a keyboard poweruser typing

# `waft-launcher` + `waft-plugin-niri`

The launcher should be able to search in niri windows using the niri plugin entities.

# `waft-settings` keyboard layout

The waft settings displays the active keyboard layout in keyboard settings. It looks very nice, but it is missing the key line above "qwerty...", which is evry important for example for differences between CZ an EN layouts.

# `waft-settings` online accounts

The online accounts are too crowded. Please make each of the account items top line a link to the account detail page with all the settings switches. Please give each of the account types an icon, for example Google row item should be prepended with Google logo. Flip the row - the email address (username / account name like) should be on top and account type on bottom.
