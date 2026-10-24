# [env] requires 1.47.0
set minimum-version := "1.47.0"

# Some commands require a .env file with CHIBI_ISO set to the ISO path
set dotenv-load

out_dir := justfile_directory() / "out"
vsix_path := out_dir / "unplug-vscode.vsix"

# Build and run Unplug
[no-exit-message]
[positional-arguments]
run *args="":
    @cargo run -p unplug-cli -q -- "$@"

alias unplug := run

# Default build
[positional-arguments]
build *args="":
    cargo build "$@"

# Distribution build (requires nightly toolchain)
[arg("toolchain", short="t", long)]
[env("RUSTFLAGS", "-C strip=symbols")]
build-dist toolchain="nightly" *args="":
    cargo +{{toolchain}} build -p unplug-cli \
        --config .cargo/distribution.toml \
        --release \
        --no-default-features \
        --features distribution \
        -Z build-std=std,panic_abort \
        -Z panic-immediate-abort \
        {{args}}

alias dist := build-dist

# Build the vscode extension
build-vsix:
    @mkdir -p "{{out_dir}}"
    cd unplug-vscode && vsce package \
        --allow-missing-repository \
        --skip-license \
        -o "{{vsix_path}}"

alias vsix := build-vsix

# Clean outputs
clean:
    @cargo clean -q
    @rm -f "{{vsix_path}}"

# Run unit tests (no ISO required)
test *tests:
    cargo test --lib -- {{tests}}

# Run all tests, including ISO tests (requires CHIBI_ISO in .env)
test-iso *tests:
    @echo "Using ISO: {{env("CHIBI_ISO")}}"
    cargo test -- {{tests}}

# Check all code with clippy
clippy:
    cargo clippy --workspace --all-targets

alias lint := clippy

# Run unplug-datagen (requires CHIBI_ISO in .env)
datagen:
    cargo run -p unplug-datagen -q -- "{{env("CHIBI_ISO")}}"
