# Notification toast bubbles

Think of having the toasts as unobtrusive bubbles

# Meeting invite

Whenever an email with meeting invite is received, present it as calendar notification. Provide actions to accept, decline, maybe and open calendar.

# SNI tray

Consider having the SNI tray in the waft overview.
(Status Notifier Items)

---

# `waft-launcher` response too slow

Investigate why is the response time so slow and fix it. It should be at least as fast as wofi/rofi.

## Scenario

Type short string into search input, for example "vol".

The result: The launcher renders results for "v" by the time "vol" has been typed, then renders resutls for "vo" and then renders results for "vol", which looks awkward and unstable.

Expected result: Before the user manages to type in "o", the view should rerender and provide results. It must be faster than a keyboard poweruser typing

# `waft-launcher` search

The launcher needs to search by both the localized AND the original (EN) name. Prioritize the localized name.
