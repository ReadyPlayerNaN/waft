## 1. NotificationCard fixes

- [x] 1.1 Add `hidden` flag (Rc<RefCell<bool>>) to NotificationCard struct
- [x] 1.2 Add hidden flag guard to left-click gesture handler before `widget.pick()` call
- [x] 1.3 Add hidden flag guard to right-click gesture handler
- [x] 1.4 Set `hidden = true` in gesture handlers before starting hide animation
- [x] 1.5 Wrap widget removal in revealer callback with `idle_add_local_once`

## 2. ToastWidget fixes

- [x] 2.1 Wrap widget removal in revealer callback with `idle_add_local_once`
- [x] 2.2 Verify hidden flag guards already exist in all gesture handlers

## 3. NotificationGroup fixes

- [x] 3.1 Review card removal logic in notification_group.rs to ensure it only hides, not removes
- [x] 3.2 Remove any direct `parent_box.remove()` calls outside of revealer callbacks

## 4. Verification

- [x] 4.1 Test notification card dismissal via left-click (no GTK CRITICAL errors)
- [x] 4.2 Test notification card dismissal via right-click (no GTK CRITICAL errors)
- [x] 4.3 Test toast dismissal via click (no GTK CRITICAL errors)
- [x] 4.4 Test toast dismissal via TTL expiration (no GTK CRITICAL errors)
- [x] 4.5 Test rapid repeated dismissals (no race conditions)
