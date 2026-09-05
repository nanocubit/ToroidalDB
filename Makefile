.PHONY: build check test clippy fmt fmt-check bench release clean

# Сборка всех таргетов
build:
	cargo build --all-targets

# Быстрая проверка компиляции без линковки
check:
	cargo check --all-targets

# Тесты (юнит + интеграционные)
test:
	cargo test

# Линтер
clippy:
	cargo clippy --all-targets

# Форматирование
fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

# Бенчмарки (criterion)
bench:
	cargo bench

# Релизная сборка
release:
	cargo build --release --all-targets

clean:
	cargo clean
