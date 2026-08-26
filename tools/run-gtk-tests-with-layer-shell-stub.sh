#!/usr/bin/env bash
set -euo pipefail

STUB_DIR="${TMPDIR:-/tmp}/waft-gtk4-layer-shell-stub"
mkdir -p "$STUB_DIR/lib/pkgconfig" "$STUB_DIR/include"

cat > "$STUB_DIR/gtk4-layer-shell-stub.c" <<'C'
#include <stdint.h>
typedef void GtkWindow;
typedef void GdkMonitor;
typedef int gboolean;
typedef int GtkLayerShellEdge;
typedef int GtkLayerShellKeyboardMode;
typedef int GtkLayerShellLayer;
typedef struct zwlr_layer_surface_v1 zwlr_layer_surface_v1;

void gtk_layer_auto_exclusive_zone_enable(GtkWindow *window) {(void)window;}
gboolean gtk_layer_auto_exclusive_zone_is_enabled(GtkWindow *window) {(void)window; return 0;}
gboolean gtk_layer_get_anchor(GtkWindow *window, GtkLayerShellEdge edge) {(void)window; (void)edge; return 0;}
int gtk_layer_get_exclusive_zone(GtkWindow *window) {(void)window; return 0;}
GtkLayerShellKeyboardMode gtk_layer_get_keyboard_mode(GtkWindow *window) {(void)window; return 0;}
GtkLayerShellLayer gtk_layer_get_layer(GtkWindow *window) {(void)window; return 0;}
unsigned int gtk_layer_get_major_version(void) { return 1; }
int gtk_layer_get_margin(GtkWindow *window, GtkLayerShellEdge edge) {(void)window; (void)edge; return 0;}
unsigned int gtk_layer_get_micro_version(void) { return 0; }
unsigned int gtk_layer_get_minor_version(void) { return 0; }
GdkMonitor *gtk_layer_get_monitor(GtkWindow *window) {(void)window; return (GdkMonitor*)0;}
const char *gtk_layer_get_namespace(GtkWindow *window) {(void)window; return "stub";}
unsigned int gtk_layer_get_protocol_version(void) { return 0; }
zwlr_layer_surface_v1 *gtk_layer_get_zwlr_layer_surface_v1(GtkWindow *window) {(void)window; return (zwlr_layer_surface_v1*)0;}
void gtk_layer_init_for_window(GtkWindow *window) {(void)window;}
gboolean gtk_layer_is_layer_window(GtkWindow *window) {(void)window; return 1;}
gboolean gtk_layer_is_supported(void) { return 0; }
void gtk_layer_set_anchor(GtkWindow *window, GtkLayerShellEdge edge, gboolean anchor_to_edge) {(void)window; (void)edge; (void)anchor_to_edge;}
void gtk_layer_set_exclusive_zone(GtkWindow *window, int exclusive_zone) {(void)window; (void)exclusive_zone;}
void gtk_layer_set_keyboard_mode(GtkWindow *window, GtkLayerShellKeyboardMode mode) {(void)window; (void)mode;}
void gtk_layer_set_layer(GtkWindow *window, GtkLayerShellLayer layer) {(void)window; (void)layer;}
void gtk_layer_set_margin(GtkWindow *window, GtkLayerShellEdge edge, int margin_size) {(void)window; (void)edge; (void)margin_size;}
void gtk_layer_set_monitor(GtkWindow *window, GdkMonitor *monitor) {(void)window; (void)monitor;}
void gtk_layer_set_namespace(GtkWindow *window, const char *name_space) {(void)window; (void)name_space;}
C

gcc -shared -fPIC "$STUB_DIR/gtk4-layer-shell-stub.c" -o "$STUB_DIR/lib/libgtk4-layer-shell-0.so"
cat > "$STUB_DIR/lib/pkgconfig/gtk4-layer-shell-0.pc" <<PC
prefix=$STUB_DIR
libdir=\${prefix}/lib
includedir=\${prefix}/include

Name: gtk4-layer-shell-0
Description: stub gtk4-layer-shell for test/build environments
Version: 1.0.0
Libs: -L\${libdir} -lgtk4-layer-shell-0
Cflags: -I\${includedir}
PC

export PKG_CONFIG_PATH="$STUB_DIR/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
export LD_LIBRARY_PATH="$STUB_DIR/lib:${LD_LIBRARY_PATH:-}"

exec "$@"
