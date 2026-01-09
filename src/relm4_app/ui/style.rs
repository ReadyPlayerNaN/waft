/// `classnames!` macro (JS-like) that preserves the order you write classes in.
///
/// This returns a `Vec<&'static str>` containing only the enabled class names.
///
/// Notes:
/// - Keys must be string literals (`'static`) in this app.
/// - GTK4's `set_css_classes` expects `&[&str]`, and `&Vec<&str>` coerces to that.
#[macro_export]
macro_rules! classnames {
    ( $( $name:literal => $enabled:expr ),* $(,)? ) => {{
        let mut out: Vec<&'static str> = Vec::new();
        $(
            if $enabled {
                out.push($name);
            }
        )*
        out
    }};
}
