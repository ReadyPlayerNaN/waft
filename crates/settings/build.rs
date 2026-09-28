use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=ui/settings.gresource.xml");
    println!("cargo:rerun-if-changed=ui");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let output = out_dir.join("settings.gresource");
    let status = Command::new("glib-compile-resources")
        .arg("ui/settings.gresource.xml")
        .arg("--sourcedir=ui")
        .arg(format!("--target={}", output.display()))
        .status()
        .expect("glib-compile-resources must be installed to build waft-settings");

    assert!(
        status.success(),
        "glib-compile-resources failed while compiling settings UI resources"
    );
}
