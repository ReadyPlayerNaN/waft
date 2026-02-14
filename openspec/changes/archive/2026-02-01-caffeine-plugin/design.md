## Context

Sacrebleui is a Wayland overlay panel with a plugin system. Feature toggles appear in a grid and allow users to enable/disable functionality. The caffeine plugin needs to work across different Wayland compositors (GNOME, KDE, Sway, Hyprland) without compositor-specific code.

No single D-Bus interface for screen inhibition is universally supported:
- `org.freedesktop.portal.Inhibit` - Modern standard, works on GNOME, KDE Plasma (Wayland), Sway, niri, Hyprland (via xdg-desktop-portal)
- `org.freedesktop.ScreenSaver` - Legacy X11 interface, works on KDE/XFCE
- Wayland `idle-inhibit` protocol - Native compositor protocol, requires surface-based integration (architectural complexity)

## Goals / Non-Goals

**Goals:**
- Provide a feature toggle to inhibit screen lock/screensaver
- Support multiple D-Bus backends with automatic detection
- Hide the toggle on systems where no backend is available
- Follow existing plugin patterns (darkman as reference)

**Non-Goals:**
- Complete Wayland `idle-inhibit` protocol implementation with surface-based inhibition (architectural complexity mixing GTK and wayland-client)
- Compositor-specific implementations
- Automatic caffeine mode based on activity detection

## Decisions

### 1. Backend Detection Strategy

**Decision**: Probe interfaces during `init()`, fail plugin initialization if none available.

**Alternatives considered**:
- Lazy detection on first toggle → Poor UX if user toggles and nothing happens
- Always show toggle with error state → Confusing when inhibit never works

**Rationale**: Failing `init()` means the plugin doesn't register, so the toggle never appears. This matches user expectation: "hide when unsupported."

### 2. Interface Priority Order

**Decision**: Detect Wayland environment first, then try Portal, then ScreenSaver.

1. Wayland detection via environment variables and GDK display type
2. `org.freedesktop.portal.Inhibit` (flag 8 = Idle) - primary backend for Wayland
3. `org.freedesktop.ScreenSaver.Inhibit` - legacy X11 fallback

**Rationale**:
- Wayland detection helps identify compositor type for logging and diagnostics
- Portal works across GNOME, KDE Plasma (Wayland), Sway, niri, Hyprland via xdg-desktop-portal
- Automatic Portal fallback provides working inhibition on Wayland compositors
- ScreenSaver remains available for legacy X11 systems

### 3. Inhibit Handle Management

**Decision**: Store inhibit handle/cookie in the D-Bus module, not in the store.

**Rationale**: The store should contain UI-relevant state (active, busy). The cookie is an implementation detail of the D-Bus layer. This follows the darkman pattern where D-Bus state stays in the dbus module.

### 4. Probing Method

**Decision**: Use lightweight D-Bus calls to detect interface availability.

- Portal: Introspect or ping the service
- ScreenSaver: Call `GetActive()` (read-only, safe)

**Rationale**: Actual inhibit calls could have side effects. Read-only methods confirm the interface exists without changing system state.

## Risks / Trade-offs

**[Risk] Portal may be available but not functional** → Accept this limitation. Portal presence doesn't guarantee the compositor implements idle inhibition. Users on such systems will see the toggle but inhibition may not work. This is rare in practice.

**[Risk] ScreenSaver interface path varies** → Try both `/ScreenSaver` and `/org/freedesktop/ScreenSaver` paths during probing.

**[Trade-off] Wayland native protocol incomplete** → Detecting Wayland and establishing protocol connection without creating surface-based inhibitors. The protocol requires a `wl_surface` from GTK, and mixing GTK's Wayland objects with wayland-client's event loop is architecturally complex. The automatic Portal fallback ensures working inhibition on Wayland compositors (Sway, niri, Hyprland) via xdg-desktop-portal.

**[Enhancement] Wayland compositor support** → Successfully supports Sway, niri, Hyprland, and other wlroots-based compositors via Portal backend, which works through xdg-desktop-portal. This provides broader Wayland support than initially planned.
