# Online Accounts -- Add New Account Implementation Plan

## Current State Summary

**Current behavior:** The online accounts page (`crates/settings/src/pages/online_accounts.rs`) has an "Add Account..." button that simply spawns `gnome-control-center online-accounts` as a subprocess. This is a crude workaround that:
1. Requires GNOME Settings to be installed
2. Opens a completely separate application
3. Does not integrate with the waft-settings navigation

**Existing infrastructure:**
- The GOA plugin (`plugins/gnome-online-accounts/`) monitors D-Bus signals for `InterfacesAdded`/`InterfacesRemoved`/`PropertiesChanged`, so any account added by any mechanism automatically appears in the entity store
- The protocol entity (`crates/protocol/src/entity/accounts.rs`) defines `OnlineAccount` with provider info, services, and status
- The settings page already subscribes and reconciles dynamically
- Localization keys `online-accounts-add-account` exist in both en-US and cs-CZ

**GOA D-Bus API:**
- `/org/gnome/OnlineAccounts/Manager` exposes `org.gnome.OnlineAccounts.Manager`
- `AddAccount(provider, identity, presentation_identity, credentials, details) -> account_path` -- low-level, requires pre-obtained credentials
- `IsSupportedProvider(provider_type) -> is_supported` -- checks if provider available
- Provider types: `"google"`, `"ms365"`, `"owncloud"`, `"imap_smtp"`, `"exchange"`, `"kerberos"`, `"fedora"`

**GOA Backend Library (`libgoa-backend-1.0`):**
- `goa_provider_get_all()` lists all available providers
- `goa_provider_add_account()` takes a GTK parent widget and drives the full add-account UI flow (OAuth WebKit dialogs for Google/Microsoft, form-based for IMAP/SMTP)
- No GIR/Rust bindings exist for GoaBackend -- only for Goa (client-side D-Bus)

---

## Key Architectural Decision Point

### Approach A: Use `libgoa-backend-1.0` C FFI from waft-settings (Recommended)

Call `goa_provider_get_all()` to list providers, then `goa_provider_add_account()` with waft-settings window as parent. GOA creates its own dialog (WebKit for OAuth, forms for IMAP).

**Pros:**
- Exact same flow as GNOME Settings
- Full OAuth support with proper WebKit token handling
- Handles credential storage via GNOME Keyring
- New providers automatically supported when GOA updates

**Cons:**
- Requires unsafe C FFI (no Rust bindings for goa-backend)
- Adds `libgoa-backend-1.0` system dependency
- `goa_provider_add_account` creates its own dialog -- cannot be customized or embedded in libadwaita navigation
- WebKit dependency transitively pulled in

### Approach B: Custom OAuth + D-Bus `Manager.AddAccount`

Expose providers from plugin, open browser for OAuth, present forms for IMAP, call `Manager.AddAccount` with credentials.

**Pros:** Pure D-Bus, no C FFI, full UI control.

**Cons:** Extremely complex. Each provider needs custom handling. GOA uses hardcoded client IDs for Google/Microsoft -- replicating is fragile. Very high effort, essentially reimplementing goa-backend.

### Recommendation: Approach A with minimal scope

---

## Implementation Plan

### Phase 0: Immediate Improvement (Low effort)

Replace `gnome-control-center` spawn with a hardcoded provider list dialog. When user selects a provider, use D-Bus `Manager.AddAccount` for simple providers (IMAP/SMTP with user-provided credentials). For OAuth providers, show a message that they need GNOME Settings. Partial progress, no new dependencies.

### Phase 1: Full GOA Integration (Medium effort)

#### Step 1: Add goa-backend FFI to waft-settings

Add minimal unsafe FFI bindings since the `goa-sys` crate is very old. Needed functions:
- `goa_provider_get_all()` / `goa_provider_get_all_finish()` -- list providers
- `goa_provider_get_provider_type()` -- get type string
- `goa_provider_get_provider_name()` -- get display name
- `goa_provider_get_provider_icon()` -- get icon
- `goa_provider_add_account()` / `goa_provider_add_account_finish()` -- run add flow

#### Step 2: Create provider picker dialog

In `crates/settings/src/online_accounts/add_account_dialog.rs`:
- `adw::Dialog` showing list of available GOA providers
- Each row shows icon, name
- On selection, call `goa_provider_add_account()` with dialog as parent
- GOA takes over with its own sub-dialog

#### Step 3: Replace gnome-control-center spawn

In `crates/settings/src/pages/online_accounts.rs`, replace the subprocess spawn with new dialog launch.

#### Step 4: Handle post-add

After `goa_provider_add_account_finish()` completes, the account is already created in GOA. The existing D-Bus signal monitor in the plugin detects `InterfacesAdded` and emits the new entity automatically. No additional protocol work needed.

### Phase 2: Provider Entity Type (Future extensibility)

1. New entity type `online-account-provider` in protocol
2. Plugin enumerates supported providers via `IsSupportedProvider` D-Bus calls
3. Other apps can also enumerate available providers

---

## Protocol Changes

**Phase 0-1:** None required. Existing `InterfacesAdded` signal monitoring handles new accounts.

**Phase 2:**
- New entity type `online-account-provider` with `provider_type`, `provider_name`, `icon_name` fields
- New action `add-account` on provider entities

## Plugin Changes

**Phase 0-1:** None. Plugin already monitors for new accounts.

**Phase 2:** Enumerate supported providers, emit `online-account-provider` entities.

## UI Changes

1. Replace `gnome-control-center` subprocess spawn with provider picker dialog
2. Create `crates/settings/src/online_accounts/add_account_dialog.rs`
3. Add localization keys for provider names and picker dialog
4. Handle GOA add-account flow result (success/cancel/error)

---

## Questions Requiring User Input

1. **C FFI vs subprocess:** Are you comfortable adding C FFI (`libgoa-backend-1.0`) as a dependency of waft-settings? The alternative is keeping the subprocess approach with a more targeted command.

2. **Provider list source:**
   - (a) Hardcoded known list with `IsSupportedProvider` checks via D-Bus (simpler)
   - (b) Dynamic enumeration via `goa_provider_get_all()` C FFI (most accurate)
   - (c) A new entity type from the plugin

3. **Scope of add-account UI:**
   - (a) Minimal: just open GOA's built-in dialog -- similar to gnome-control-center
   - (b) Full: build custom provider-type-specific forms -- massive effort

4. **GOA daemon unavailable:** What should happen if `goa-daemon` is not running? Hide "Add Account" button or show an error?

5. **Feature flag:** Should goa-backend integration be behind a cargo feature flag to keep waft-settings buildable without `libgoa-backend` installed?

---

## Critical Files

- `crates/settings/src/pages/online_accounts.rs` - Add button handler to replace
- `plugins/gnome-online-accounts/src/dbus.rs` - GOA D-Bus constants and operations
- `crates/protocol/src/entity/accounts.rs` - Entity types (if adding provider entity)
- `crates/settings/src/online_accounts/account_detail.rs` - Pattern reference for sub-page dialogs
- `crates/settings/src/wifi/password_dialog.rs` - Pattern reference for inline dialogs
