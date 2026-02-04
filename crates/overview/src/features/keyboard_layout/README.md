# Keyboard Layout Plugin

Multi-backend keyboard layout indicator and switcher for Sacrebleui.

## Overview and Features

The keyboard layout plugin provides quick access to view and switch keyboard layouts directly from the Sacrebleui overlay interface.

**Features:**
- Displays current keyboard layout as an uppercase abbreviation (e.g., "US", "DE", "FR")
- Click-to-cycle through configured keyboard layouts
- **Live updates**: Automatically updates when layout changes externally (e.g., keyboard shortcuts)
- **Multi-backend support**: Niri, Sway, Hyprland, and systemd-localed
- **Automatic backend detection** based on environment variables
- Graceful degradation when no backend is available
- Keyboard navigation support (Tab to focus, Enter/Space to cycle)
- Accessible with proper ARIA labels

## Backend Support

The plugin automatically detects and uses the appropriate backend for your compositor:

| Backend | Detection | Event Source | Live Updates |
|---------|-----------|--------------|--------------|
| **Niri** | `NIRI_SOCKET` env var | `niri msg event-stream` | Yes |
| **Sway** | `SWAYSOCK` env var | `swaymsg -t subscribe '["input"]'` | Yes |
| **Hyprland** | `HYPRLAND_INSTANCE_SIGNATURE` env var | Socket at `.socket2.sock` | Yes |
| **systemd-localed** | D-Bus locale1 available | D-Bus PropertiesChanged | Config only* |

*systemd-localed only notifies on configuration changes, not runtime layout switches.

### Detection Order

The plugin checks for backends in this order:
1. Niri (if `NIRI_SOCKET` is set)
2. Sway (if `SWAYSOCK` is set)
3. Hyprland (if `HYPRLAND_INSTANCE_SIGNATURE` is set)
4. systemd-localed (if D-Bus locale1 service is available)

If no backend is available, the widget displays "??" and is non-functional.

## Compositor-Specific Setup

### Niri

Niri provides excellent IPC support. Simply configure your keyboard layouts in your Niri config:

```kdl
input {
    keyboard {
        xkb {
            layout "us,cz"
        }
    }
}
```

The plugin will automatically query layouts via `niri msg --json keyboard-layouts`.

### Sway

Configure keyboard layouts in your Sway config:

```
input type:keyboard {
    xkb_layout "us,de,fr"
}
```

The plugin queries layouts via `swaymsg -t get_inputs` and switches via `swaymsg input type:keyboard xkb_switch_layout next`.

### Hyprland

Configure keyboard layouts in your Hyprland config:

```
input {
    kb_layout = us,de
}
```

The plugin queries the active layout via `hyprctl devices -j` and switches via `hyprctl switchxkblayout all next`.

### systemd-localed (Fallback)

If no compositor-specific backend is available, the plugin falls back to systemd-localed via D-Bus.

#### Service Information

- **D-Bus Service**: `org.freedesktop.locale1`
- **D-Bus Interface**: `org.freedesktop.locale1`
- **Object Path**: `/org/freedesktop/locale1`
- **Property**: `X11Layout` (comma-separated list of configured XKB layouts)
- **Method**: `SetX11Keyboard` (change keyboard layout)

#### Checking if systemd-localed is Available

```bash
systemctl status systemd-localed
# or
busctl status org.freedesktop.locale1
```

#### Configuring Layouts

```bash
# View current configuration
localectl status

# Set single layout
localectl set-x11-keymap us

# Set multiple layouts
localectl set-x11-keymap us,de,fr
```

## Configuration

Enable the plugin in `~/.config/waft-overview/config.toml`:

```toml
[[plugins]]
id = "plugin::keyboard-layout"
```

## Layout Abbreviations

The plugin extracts short abbreviations from layout names:

| Full Name | Abbreviation |
|-----------|--------------|
| English (US) | US |
| Czech (QWERTY) | CZ |
| German | DE |
| French | FR |
| us | US |

The extraction logic:
1. If the name contains parentheses with a short code (2-4 chars), use that: "English (US)" → "US"
2. Otherwise, map the language name to a country code: "Czech" → "CZ", "German" → "DE"
3. Fallback: use first 2 characters uppercase

## Troubleshooting

### Widget Not Appearing

**Possible Causes:**
1. Plugin not enabled in configuration
2. No backend available (compositor not detected, systemd-localed not running)
3. No keyboard layouts configured

**Solutions:**

1. **Enable the plugin** in config.toml (see Configuration section above)

2. **Check which backend is detected** in application logs:
   ```bash
   waft-overview 2>&1 | grep keyboard-layout
   ```
   You should see a message like:
   ```
   [keyboard-layout] Detected Niri compositor, using Niri backend
   ```
   or
   ```
   [keyboard-layout] No backend available, plugin will show fallback indicator
   ```

3. **For Niri/Sway/Hyprland**: Ensure your compositor is running and the environment variable is set

4. **For systemd-localed fallback**: Start the service:
   ```bash
   sudo systemctl start systemd-localed
   ```

### Layouts Not Switching

**Possible Causes:**
1. Compositor command failed
2. PolicyKit authorization required (systemd-localed only)
3. Invalid layout configuration

**Solutions:**

1. **Test compositor commands manually**:
   - Niri: `niri msg action switch-layout next`
   - Sway: `swaymsg input type:keyboard xkb_switch_layout next`
   - Hyprland: `hyprctl switchxkblayout all next`

2. **PolicyKit Authorization** (systemd-localed): Check PolicyKit configuration for org.freedesktop.locale1

3. **Check logs** for detailed error messages

### Fallback Indicator ("??")

If the widget displays "??" instead of a layout abbreviation:

1. **No backend available**: Check that your compositor is running or systemd-localed is available
2. **No layouts configured**: Configure keyboard layouts in your compositor config
3. **Command execution failed**: The compositor command (niri/swaymsg/hyprctl) might not be in PATH

## Testing

### Unit Tests

Run unit tests for all backends:

```bash
cargo test keyboard_layout
```

This runs tests for:
- JSON parsing for Niri, Sway, and Hyprland responses
- XKB layout parsing for systemd-localed
- Abbreviation extraction from layout names
- Language-to-country code mapping

## Architecture

### Components

```
src/features/keyboard_layout/
├── mod.rs                 # Plugin implementation
├── widget.rs              # GTK widget
├── backends/
│   ├── mod.rs             # Backend trait + detection + abbreviation helpers
│   ├── niri.rs            # Niri IPC backend
│   ├── sway.rs            # Sway IPC backend
│   ├── hyprland.rs        # Hyprland IPC backend
│   └── localed.rs         # systemd-localed D-Bus backend
└── README.md
```

### Backend Trait

All backends implement the `KeyboardLayoutBackend` trait:

```rust
#[async_trait]
pub trait KeyboardLayoutBackend: Send + Sync {
    async fn get_layout_info(&self) -> Result<LayoutInfo>;
    async fn switch_next(&self) -> Result<()>;
    async fn switch_prev(&self) -> Result<()>;
    fn name(&self) -> &'static str;
    fn subscribe(&self, sender: Sender<LayoutEvent>);
}

pub struct LayoutInfo {
    pub current: String,      // e.g., "US"
    pub available: Vec<String>, // e.g., ["US", "CZ"]
    pub current_index: usize,
}

pub enum LayoutEvent {
    Changed(LayoutInfo),
    Error(String),
}
```

The `subscribe` method spawns a background task that monitors for layout changes and sends events through the provided channel. This enables live updates when the layout changes externally (e.g., via keyboard shortcuts).

### Runtime Bridging

The plugin bridges two async runtimes:
- **GTK/glib**: UI event loop (glib::spawn_future_local)
- **tokio**: Backend operations (crate::runtime::spawn_on_tokio)

**Pattern:**
```rust
glib::spawn_future_local(async move {
    let result = crate::runtime::spawn_on_tokio(async move {
        // Backend call here (tokio context)
        backend.switch_next().await
    }).await;

    // Update UI here (glib context)
    label.set_label(&new_layout);
});
```

This pattern ensures backend calls don't block the GTK main thread while maintaining proper async execution.
