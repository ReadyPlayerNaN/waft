# Arch Linux PKGBUILD Packaging Plan

## Context

Waft is currently distributed via a single `waft-overview-git` PKGBUILD that only packages the overview application. This plan creates a comprehensive packaging structure to deliver the complete Waft ecosystem to Arch Linux users with fine-grained package splits, allowing users to install only the components they need while providing a convenient meta-package for common desktop use cases.

**Goals:**
- Enable modular installation (core, apps, and plugins as separate packages)
- Provide `waft-desktop` meta-package for convenient "batteries included" installation
- Follow Arch Linux packaging best practices
- Support active development via git-based packages
- Allow future migration to official repositories

## Package Architecture

**Total packages**: 20 (1 core + 3 apps + 13 plugins + 1 meta-package + waft-toasts + niri)

### Core Package
- **`waft-git`**: Central daemon + D-Bus service + systemd unit + bundled plugins (clock, systemd-actions)

### Application Packages
- **`waft-overview-git`**: Main GTK4/libadwaita overlay UI
- **`waft-settings-git`**: Standalone settings application with `.desktop` file
- **`waft-toasts-git`**: Toast notification display daemon

### Plugin Packages (13 separate packages)
- **`waft-plugin-notifications-git`**: D-Bus notification server with DND support
- **`waft-plugin-audio-git`**: Volume control (pactl backend)
- **`waft-plugin-battery-git`**: Battery status (UPower D-Bus)
- **`waft-plugin-networkmanager-git`**: WiFi/Ethernet/VPN management (nmrs + zbus)
- **`waft-plugin-bluez-git`**: Bluetooth device management (BlueZ D-Bus)
- **`waft-plugin-weather-git`**: Weather information (HTTP API)
- **`waft-plugin-eds-git`**: Evolution Data Server calendar integration
- **`waft-plugin-brightness-git`**: Display brightness control (brightnessctl/ddcutil)
- **`waft-plugin-darkman-git`**: Dark mode toggle (darkman D-Bus)
- **`waft-plugin-caffeine-git`**: Sleep/screensaver inhibitor
- **`waft-plugin-keyboard-layout-git`**: Input method display/switching
- **`waft-plugin-sunsetr-git`**: Night light control (sunsetr CLI)
- **`waft-plugin-syncthing-git`**: Syncthing service toggle
- **`waft-plugin-niri-git`**: Niri window manager support

### Meta Package
- **`waft-desktop-git`**: Depends on waft + all 3 apps + 8 essential plugins (notifications, audio, networkmanager, bluez, battery, weather, eds, brightness)

## Directory Structure

```
/home/just-paja/Work/shell/sacrebleui/
├── PKGBUILD (existing - will be replaced/updated)
├── PKGBUILD/
│   ├── waft-git/PKGBUILD
│   ├── waft-overview-git/PKGBUILD
│   ├── waft-settings-git/PKGBUILD
│   ├── waft-toasts-git/PKGBUILD
│   ├── waft-plugin-notifications-git/PKGBUILD
│   ├── waft-plugin-audio-git/PKGBUILD
│   ├── waft-plugin-battery-git/PKGBUILD
│   ├── waft-plugin-networkmanager-git/PKGBUILD
│   ├── waft-plugin-bluez-git/PKGBUILD
│   ├── waft-plugin-weather-git/PKGBUILD
│   ├── waft-plugin-eds-git/PKGBUILD
│   ├── waft-plugin-brightness-git/PKGBUILD
│   ├── waft-plugin-darkman-git/PKGBUILD
│   ├── waft-plugin-caffeine-git/PKGBUILD
│   ├── waft-plugin-keyboard-layout-git/PKGBUILD
│   ├── waft-plugin-sunsetr-git/PKGBUILD
│   ├── waft-plugin-syncthing-git/PKGBUILD
│   ├── waft-plugin-niri-git/PKGBUILD
│   └── waft-desktop-git/PKGBUILD
├── data/
│   ├── org.waft.Daemon.service (existing)
│   ├── waft.service (new - systemd user unit)
│   └── waft-settings.desktop (new)
└── README.md (update with installation instructions)
```

## Critical Files

### 1. waft-git (Core Package)

**Path**: `PKGBUILD/waft-git/PKGBUILD`

**Binaries installed**:
- `/usr/bin/waft` (central daemon)
- `/usr/bin/waft-clock-daemon` (bundled plugin)
- `/usr/bin/waft-systemd-actions-daemon` (bundled plugin)

**Service files**:
- `/usr/share/dbus-1/services/org.waft.Daemon.service` (D-Bus activation)
- `/usr/lib/systemd/user/waft.service` (systemd user unit)

**Dependencies**:
- `depends=('gcc-libs')`
- `makedepends=('cargo' 'git' 'rust')`

**Key sections**:
```bash
build() {
  cd "$srcdir/$pkgname"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release --bin waft --bin waft-clock-daemon --bin waft-systemd-actions-daemon
}

package() {
  install -Dm755 "target/release/waft" "$pkgdir/usr/bin/waft"
  install -Dm755 "target/release/waft-clock-daemon" "$pkgdir/usr/bin/waft-clock-daemon"
  install -Dm755 "target/release/waft-systemd-actions-daemon" "$pkgdir/usr/bin/waft-systemd-actions-daemon"
  install -Dm644 "data/org.waft.Daemon.service" "$pkgdir/usr/share/dbus-1/services/org.waft.Daemon.service"
  install -Dm644 "data/waft.service" "$pkgdir/usr/lib/systemd/user/waft.service"
  install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
```

### 2. waft-overview-git (Main UI)

**Path**: `PKGBUILD/waft-overview-git/PKGBUILD`

**Binaries installed**:
- `/usr/bin/waft-overview`

**Dependencies**:
- `depends=('waft-git' 'gtk4' 'libadwaita' 'gtk4-layer-shell')`
- `makedepends=('cargo' 'git' 'rust')`

**Build optimization**: Use shared `CARGO_TARGET_DIR` to reuse build artifacts

### 3. waft-settings-git (Settings App)

**Path**: `PKGBUILD/waft-settings-git/PKGBUILD`

**Binaries installed**:
- `/usr/bin/waft-settings`

**Desktop integration**:
- `/usr/share/applications/waft-settings.desktop`

**Dependencies**:
- `depends=('waft-git' 'gtk4' 'libadwaita')`
- `makedepends=('cargo' 'git' 'rust')`

**Desktop file** (`data/waft-settings.desktop`):
```desktop
[Desktop Entry]
Type=Application
Name=Waft Settings
Comment=Configure Waft overlay and plugins
Exec=waft-settings
Icon=preferences-system
Terminal=false
Categories=Settings;DesktopSettings;GTK;
```

### 4. waft-toasts-git (Toast Daemon)

**Path**: `PKGBUILD/waft-toasts-git/PKGBUILD`

**Binaries installed**:
- `/usr/bin/waft-toasts`

**Dependencies**:
- `depends=('waft-git' 'gtk4' 'libadwaita' 'gtk4-layer-shell')`
- `makedepends=('cargo' 'git' 'rust')`

### 5. Plugin Packages (Template Pattern)

Each plugin follows the same structure. Example: `waft-plugin-notifications-git`

**Path**: `PKGBUILD/waft-plugin-notifications-git/PKGBUILD`

**Binaries installed**:
- `/usr/bin/waft-notifications-daemon`

**Dependencies**:
- `depends=('waft-git')` (all plugins depend on core)
- Plugin-specific system dependencies as `depends=()`:
  - notifications: no extra deps
  - audio: no extra deps (uses pactl via command)
  - battery: `depends=('waft-git' 'upower')`
  - networkmanager: `depends=('waft-git' 'networkmanager')`
  - bluez: `depends=('waft-git' 'bluez')`
  - weather: no extra deps (HTTP client built-in)
  - eds: `depends=('waft-git' 'evolution-data-server')`
  - brightness: no extra deps (calls brightnessctl/ddcutil via command)
  - darkman: `depends=('waft-git' 'darkman')` (AUR package)
  - caffeine: no extra deps (D-Bus interfaces)
  - keyboard-layout: no extra deps (compositor D-Bus)
  - sunsetr: `depends=('waft-git' 'sunsetr')` (AUR package)
  - syncthing: `depends=('waft-git' 'syncthing')`
  - niri: no extra deps (D-Bus interface)

**Build command**:
```bash
cargo build --frozen --release --bin waft-{plugin}-daemon
```

### 6. waft-desktop-git (Meta Package)

**Path**: `PKGBUILD/waft-desktop-git/PKGBUILD`

**Type**: Meta-package (no binaries, only dependencies)

**Dependencies**:
```bash
depends=(
  'waft-git'
  'waft-overview-git'
  'waft-settings-git'
  'waft-toasts-git'
  'waft-plugin-notifications-git'
  'waft-plugin-audio-git'
  'waft-plugin-networkmanager-git'
  'waft-plugin-bluez-git'
  'waft-plugin-battery-git'
  'waft-plugin-weather-git'
  'waft-plugin-eds-git'
  'waft-plugin-brightness-git'
)
```

**Package function**:
```bash
package() {
  # Meta-package - no files to install
  true
}
```

## Systemd User Unit

**Path**: `data/waft.service`

```ini
[Unit]
Description=Waft overlay daemon
Documentation=https://github.com/readyplayernan/waft
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/bin/waft
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

**Purpose**: Allows users to enable persistent daemon with `systemctl --user enable waft.service`

**Note**: D-Bus activation (`org.waft.Daemon.service`) remains the primary startup method. Systemd unit is optional for users who want explicit control.

## Build Optimization Strategy

### Shared Cargo Cache

All PKGBUILDs will use a shared `CARGO_TARGET_DIR` to avoid rebuilding the entire workspace for each package:

```bash
build() {
  cd "$srcdir/$pkgname"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}"
  cargo build --frozen --release --bin <specific-binary>
}
```

**Benefits**:
- Workspace builds once, dependencies cached
- Subsequent package builds reuse compiled crates
- Reduces total build time from ~20x to ~1.5x workspace build time

### Parallel Builds

Users can build multiple packages in parallel:
```bash
cd PKGBUILD
for dir in waft-plugin-*/; do (cd "$dir" && makepkg -si) & done
wait
```

## Installation Paths

All binaries install to `/usr/bin/` (the default plugin discovery path):
- `/usr/bin/waft` (daemon)
- `/usr/bin/waft-overview` (main UI)
- `/usr/bin/waft-settings` (settings UI)
- `/usr/bin/waft-toasts` (toast daemon)
- `/usr/bin/waft-{plugin}-daemon` (15 plugins)

Service files:
- `/usr/share/dbus-1/services/org.waft.Daemon.service`
- `/usr/lib/systemd/user/waft.service`

Desktop files:
- `/usr/share/applications/waft-settings.desktop`

Documentation:
- `/usr/share/doc/{package}/README.md`
- `/usr/share/licenses/{package}/LICENSE`

## Common PKGBUILD Template Structure

All PKGBUILDs share this structure (adapted per package):

```bash
pkgname=<package>-git
pkgver=r<commits>.<hash>
pkgrel=1
pkgdesc="<description>"
arch=('x86_64' 'aarch64')
url="https://github.com/readyplayernan/waft"
license=('MIT')
depends=(<dependencies>)
makedepends=('cargo' 'git' 'rust')
provides=('<package>')
conflicts=('<package>')
source=("git+https://github.com/readyplayernan/waft.git")
sha256sums=('SKIP')

pkgver() {
  cd "$srcdir/waft"
  printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
  cd "$srcdir/waft"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$srcdir/waft"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release --bin <binary-name>
}

check() {
  cd "$srcdir/waft"
  export RUSTUP_TOOLCHAIN=stable
  cargo test --frozen --bin <binary-name> || true  # Tests optional for plugins
}

package() {
  cd "$srcdir/waft"
  install -Dm755 "target/release/<binary>" "$pkgdir/usr/bin/<binary>"
  install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  # Additional files as needed
}
```

## Verification Plan

### Build Testing

1. **Individual package build**:
```bash
cd PKGBUILD/waft-git
makepkg -sf
```

2. **Dependency chain test**:
```bash
# Build and install in order
cd PKGBUILD/waft-git && makepkg -si
cd PKGBUILD/waft-overview-git && makepkg -si
cd PKGBUILD/waft-plugin-notifications-git && makepkg -si
```

3. **Meta-package test**:
```bash
cd PKGBUILD/waft-desktop-git && makepkg -si
# Should pull in all dependencies
```

### Runtime Testing

1. **Daemon startup**:
```bash
# D-Bus activation test
systemctl --user status waft  # Should show inactive
waft-overview &
systemctl --user status waft  # Should show active (started by D-Bus)

# Systemd unit test
systemctl --user enable --now waft
systemctl --user status waft
```

2. **Plugin discovery**:
```bash
# Check daemon logs for plugin discovery
journalctl --user -u waft -f
# Should show: discovered plugins, provides checks, spawning on demand
```

3. **UI functionality**:
```bash
waft-overview  # Launch main overlay
waft-settings  # Should appear in app menu and launch from command
```

4. **Dependency verification**:
```bash
pactree waft-desktop-git  # Should show full dependency tree
pacman -Qi waft-git        # Check installed files
```

### Package Quality Checks

1. **namcap verification** (Arch package linter):
```bash
namcap PKGBUILD/waft-git/PKGBUILD
namcap waft-git-*.pkg.tar.zst
```

2. **File conflicts**:
```bash
# Ensure no file is installed by multiple packages
for pkg in PKGBUILD/*/; do
  (cd "$pkg" && makepkg --packagelist)
done | sort | uniq -d  # Should be empty
```

3. **Binary verification**:
```bash
# All plugins should be discoverable
ls /usr/bin/waft-*-daemon
# Each should respond to provides
waft-clock-daemon provides
```

## Migration Plan

### Phase 1: Create PKGBUILDs
1. Create `PKGBUILD/` directory structure
2. Write all 20 PKGBUILDs following the template
3. Create `data/waft.service` (systemd unit)
4. Create `data/waft-settings.desktop`
5. Test build each package individually

### Phase 2: Documentation
1. Update README.md with installation instructions
2. Add `PKGBUILD/README.md` explaining package structure
3. Document dependency relationships

### Phase 3: AUR Publication
1. Create AUR repositories for each package
2. Publish `waft-git` first (core dependency)
3. Publish app packages
4. Publish plugin packages
5. Publish `waft-desktop-git` meta-package last

### Phase 4: Validation
1. Test installation via AUR helpers (yay, paru)
2. Verify dependency resolution
3. Runtime testing on clean Arch installation
4. Community feedback and iteration

## Success Criteria

- ✅ All 20 packages build successfully
- ✅ `waft-desktop-git` pulls in correct dependencies
- ✅ Daemon starts via D-Bus activation
- ✅ Daemon can be managed via systemd user unit
- ✅ Plugin discovery finds all installed plugins
- ✅ waft-settings appears in application menu
- ✅ No file conflicts between packages
- ✅ namcap validation passes
- ✅ Users can install subset of plugins as needed
