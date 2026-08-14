// build.rs — БЕЗ ЗАВИСИМОСТЕЙ
fn main() {
    // Устанавливаем заглушку для времени сборки
    println!("cargo:rustc-env=VERGEN_BUILD_TIMESTAMP=unknown");
}