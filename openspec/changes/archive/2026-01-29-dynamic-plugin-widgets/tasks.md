## 1. Core Infrastructure

- [x] 1.1 Add `id: String` field to `Widget` struct in `src/plugin.rs`
- [x] 1.2 Add `id: String` field to `WidgetFeatureToggle` struct in `src/plugin.rs`
- [x] 1.3 Define `WidgetRegistrar` trait with `register_widget`, `register_feature_toggle`, `unregister_widget`, `unregister_feature_toggle` methods
- [x] 1.4 Add widget/toggle storage (`Vec<Arc<Widget>>`, `Vec<Arc<WidgetFeatureToggle>>`) to `PluginRegistry`
- [x] 1.5 Add subscriber list (`RefCell<Vec<Rc<dyn Fn()>>>`) to `PluginRegistry`
- [x] 1.6 Implement `subscribe_widgets` method on `PluginRegistry`
- [x] 1.7 Implement `WidgetRegistrar` trait for `PluginRegistry` (or a wrapper handle)
- [x] 1.8 Make registration methods notify subscribers after state change

## 2. Plugin Trait Changes

- [x] 2.1 Update `Plugin::create_elements()` signature to accept `Rc<dyn WidgetRegistrar>` parameter
- [x] 2.2 Remove `get_widgets()` method from `Plugin` trait
- [x] 2.3 Remove `get_feature_toggles()` method from `Plugin` trait
- [x] 2.4 Update `PluginRegistry::create_elements()` to pass registrar to plugins

## 3. Main Window Integration

- [x] 3.1 Store slot container references (`header_box`, `info_col`, `controls_col`) for synchronization
- [x] 3.2 Store widget ID on GTK widget (via `set_widget_name()`) for diffing
- [x] 3.3 Create `sync_slot` helper that diffs current children vs new widgets by ID
- [x] 3.4 Implement removal of widgets no longer in registry (only those removed)
- [x] 3.5 Implement addition of newly registered widgets (only those added)
- [x] 3.6 Implement in-place reordering using `reorder_child_after()` for order changes
- [x] 3.7 Subscribe to registry widget changes in main window initialization
- [x] 3.8 Call `sync_slot` for each slot when notified of changes

## 4. Feature Grid Integration

- [x] 4.1 Add method to `FeatureGridWidget` to sync its toggle list using diff strategy
- [x] 4.2 Store toggle ID on GTK widget for diffing
- [x] 4.3 Subscribe feature grid to registry toggle changes
- [x] 4.4 Preserve expanded menu state across synchronization (MenuStore handles state externally)

## 5. Migrate Existing Plugins

- [x] 5.1 Update `AudioPlugin` to use registrar for widget registration
- [x] 5.2 Update `ClockPlugin` to use registrar for widget registration
- [x] 5.3 Update `BatteryPlugin` to use registrar for widget registration
- [x] 5.4 Update `NetworkManagerPlugin` to use registrar for widget registration
- [x] 5.5 Update `BluetoothPlugin` to use registrar for widget registration
- [x] 5.6 Update `DarkmanPlugin` to use registrar for widget registration
- [x] 5.7 Update `SunsetrPlugin` to use registrar for widget registration
- [x] 5.8 Update `NotificationsPlugin` to use registrar for widget registration
- [x] 5.9 Update `AgendaPlugin` to use registrar for widget registration
- [x] 5.10 Update `WeatherPlugin` to use registrar for widget registration

## 6. NetworkManager Dynamic Registration

- [x] 6.1 Store `Rc<dyn WidgetRegistrar>` in `NetworkManagerPlugin` for runtime use
- [x] 6.2 Generate unique widget IDs per adapter (e.g., `networkmanager:wifi:adapter-0`)
- [x] 6.3 Register adapter widgets when adapters are discovered during init
- [x] 6.4 Subscribe to NetworkManager device-added signals
- [x] 6.5 Register new adapter widget when device-added signal received
- [x] 6.6 Subscribe to NetworkManager device-removed signals
- [x] 6.7 Unregister adapter widget and cleanup when device-removed signal received

## 7. Verification

- [ ] 7.1 Verify all existing plugins display correctly after migration
- [ ] 7.2 Test plugging in USB network adapter shows new toggle
- [ ] 7.3 Test unplugging USB network adapter removes toggle
- [ ] 7.4 Test multiple adapters of same type display independently
- [ ] 7.5 Verify menu state preserved when unrelated adapter changes
- [ ] 7.6 Verify existing widgets are not remounted when new widget is added (check widget state preservation)
- [ ] 7.7 Verify widget order changes don't cause remounting (use reorder_child_after)
