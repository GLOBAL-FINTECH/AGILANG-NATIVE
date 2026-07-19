.PHONY: build test fmt lint smoke
build:
	cargo build --workspace --release

test:
	cargo test --workspace

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

smoke:
	cargo run -p agilang-runtime-cli -- smoke
