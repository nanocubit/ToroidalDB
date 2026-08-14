use std::env;
use std::path::PathBuf;

fn main() {
    // Устанавливаем переменные окружения для Tauri
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("tauri-build-env"), "1").unwrap();
    
    // Запускаем Tauri build
    tauri_build::build()
}