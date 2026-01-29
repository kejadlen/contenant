# Run all checks
all: fmt check clippy test

# Format code
fmt:
    cargo fmt

# Check code compiles
check:
    cargo check

# Run tests
test:
    cargo test

# Run clippy lints
clippy:
    cargo clippy -- -D warnings
