fmt:
	cargo fix --all-features --all-targets --allow-dirty --allow-no-vcs --allow-staged --workspace
	cargo fmt

test:
	cargo clippy --allow-dirty --allow-no-vcs --allow-staged --fix
	cargo fmt --check
	cargo test --all-features --all-targets --workspace

