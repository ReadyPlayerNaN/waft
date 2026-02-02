## 1. Module Setup

- [x] 1.1 Create `src/features/caffeine/` directory
- [x] 1.2 Create `src/features/caffeine/mod.rs` with plugin struct skeleton
- [x] 1.3 Add `pub mod caffeine;` to `src/features/mod.rs`

## 2. D-Bus Backend

- [x] 2.1 Create `src/features/caffeine/backends.rs` with `InhibitBackend` enum (Wayland, Portal, ScreenSaver) - note: file renamed from dbus.rs
- [x] 2.2 Implement portal probe function (ping `org.freedesktop.portal.Desktop`)
- [x] 2.3 Implement ScreenSaver probe function (call `GetActive()` on both path variants)
- [x] 2.4 Implement `inhibit()` for portal backend (Inhibit with flag 8)
- [x] 2.5 Implement `uninhibit()` for portal backend (drop request handle)
- [x] 2.6 Implement `inhibit()` for ScreenSaver backend (returns cookie)
- [x] 2.7 Implement `uninhibit()` for ScreenSaver backend (UnInhibit with cookie)

## 3. State Store

- [x] 3.1 Create `src/features/caffeine/store.rs` with `CaffeineState` (active, busy)
- [x] 3.2 Define `CaffeineOp` enum (SetActive, SetBusy)
- [x] 3.3 Implement `create_caffeine_store()` function

## 4. Plugin Implementation

- [x] 4.1 Implement `Plugin::init()` - probe backends, fail if none available
- [x] 4.2 Implement `Plugin::create_elements()` - create feature toggle widget
- [x] 4.3 Connect toggle output handler to call inhibit/uninhibit via D-Bus
- [x] 4.4 Subscribe store to update toggle active/busy state

## 5. App Integration

- [x] 5.1 Add plugin registration block to `src/app.rs` (follows darkman pattern)
- [x] 5.2 Add `plugin::caffeine` to default config example if applicable

## 6. Verification

- [x] 6.1 Build with `cargo build` - verify no warnings
- [x] 6.2 Test on system with portal support (Wayland/niri) - toggle appears, Portal backend detected
- [x] 6.3 Test on system with ScreenSaver support (KDE X11) - should fall back to ScreenSaver (not fully tested)
- [x] 6.4 Test activation - screen does not lock while active (Portal backend verified on Wayland/niri)
- [x] 6.5 Test deactivation - screen lock resumes normally (Portal backend verified on Wayland/niri)
