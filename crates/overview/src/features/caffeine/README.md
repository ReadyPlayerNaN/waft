# Caffeine Plugin - Screen Lock Inhibition

Prevents screen locking and idle timeout when activated.

## Supported Backends

### 1. Portal (✅ Recommended for Wayland)

**D-Bus Interface:** `org.freedesktop.portal.Inhibit`

- **Status:** Fully working
- **Works on:** GNOME, KDE Plasma (Wayland), niri, Sway, Hyprland (via xdg-desktop-portal)
- **Method:** D-Bus API for idle inhibition
- **Note:** This is the **primary backend on Wayland sessions**

### 2. Wayland Native (⚠️ Incomplete)

**Protocol:** `zwp_idle_inhibit_manager_v1`

- **Status:** Partially implemented (automatically skipped)
- **Limitation:** Surface-based inhibition incomplete
- **Reason:** Architectural complexity mixing GTK's Wayland objects with wayland-client
- **Behavior:** Detects Wayland session, then falls back to Portal automatically

#### Why Incomplete?

The Wayland idle-inhibit protocol requires a `wl_surface` to create an inhibitor. Challenges include mixing GTK's Wayland objects with wayland-client's event loop. Since Portal works perfectly on Wayland compositors, this is acceptable.

### 3. ScreenSaver (✅ Legacy Fallback)

**D-Bus Interface:** `org.freedesktop.ScreenSaver`

- **Status:** Fully working
- **Works on:** KDE Plasma (X11), XFCE, older desktop environments

## How It Works on Wayland

1. Detect Wayland session
2. Skip native Wayland backend (incomplete)
3. **Automatically fall back to Portal**
4. Portal successfully prevents screen locking ✅

## Logs You'll See

On niri/Sway/Hyprland:
```
[caffeine/backends] Wayland session detected
[caffeine/backends] Skipping Wayland backend: surface-based inhibition not fully implemented
[caffeine/backends] Falling back to Portal backend (recommended for Wayland)
[caffeine/backends] Portal backend available (recommended)
[caffeine] Using backend: Portal
```

## Testing

1. Toggle caffeine ON
2. Wait past swayidle timeout
3. Screen should NOT lock ✅
4. Toggle OFF - screen locks normally

