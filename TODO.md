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

# `waft-launcher` + `waft-plugin-niri`

The launcher does not find firefox window when I type "firefox". It only finds app launcher firefox. The windows should be prefered over over launching new app.

# `waft-launcher` highlights invisible

The waft launcher highlights the input text inside the search item rows. The highlighted part of the label text should have colour, that is distinct from the rest of the text. Possibly inverse colour of the theme highlight.
