## Plugins to implement

- Tether plugin?
- SNI

## Issues to fix

Notifications plugin sometimes yells GTK critical errors when dismissing notifications from the widget

```
(sacrebleui:1711467): Gtk-CRITICAL **: 08:58:03.935: gtk_native_get_surface: assertion 'GTK_IS_NATIVE (self)' failed

(sacrebleui:1711467): Gdk-CRITICAL **: 08:58:03.935: gdk_surface_set_device_cursor: assertion 'GDK_IS_SURFACE (surface)' failed

(sacrebleui:1711467): Gtk-CRITICAL **: 08:58:03.935: gtk_native_get_surface: assertion 'GTK_IS_NATIVE (self)' failed

(sacrebleui:1711467): Gtk-CRITICAL **: 08:58:04.008: gtk_widget_compute_point: assertion 'GTK_IS_WIDGET (widget)' failed

(sacrebleui:1711467): Gtk-CRITICAL **: 08:58:04.008: gtk_widget_compute_point: assertion 'GTK_IS_WIDGET (widget)' failed

(sacrebleui:1711467): Gtk-CRITICAL **: 08:58:04.008: gtk_native_get_surface: assertion 'GTK_IS_NATIVE (self)' failed

(sacrebleui:1711467): Gdk-CRITICAL **: 08:58:04.008: gdk_surface_set_device_cursor: assertion 'GDK_IS_SURFACE (surface)' failed
```

## NetworkManager plugin enhancements

- WiFi: Support connecting to new (unsaved) networks with password prompt
- WiFi: Signal strength icon updates in toggle (currently just on/off)
