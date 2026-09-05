fn main() {
    println!("cargo:rustc-check-cfg=cfg(mobile)");
    tauri_build::build();
}
