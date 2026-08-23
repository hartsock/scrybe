# Scrybe development tasks

# Tauri uses the workspace Cargo target directory, which may be overridden by
# CARGO_TARGET_DIR or a Cargo configuration.
cargo_target_dir := `cargo metadata --manifest-path scrybe-app/src-tauri/Cargo.toml --no-deps --format-version 1 | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'`

default:
    @just --list

build:
    cargo build

release:
    cargo build --release

check:
    cargo check --all-targets
    cargo clippy --all-targets -- -D warnings
    cargo fmt -- --check

test:
    cargo test

fmt:
    cargo fmt

clean:
    cargo clean

# Full install: build app + all Python packages into ~/venv, bundle to ~/Applications
install: install-app

# Install the desktop app plus its runtime Python tools.
install-app: app install-python-toolkit
    rm -rf ~/Applications/Scrybe.app
    rm -f ~/venv/bin/scrybe-app
    mkdir -p ~/venv/bin
    cp {{cargo_target_dir}}/release/bundle/macos/Scrybe.app/Contents/MacOS/scrybe-app ~/venv/bin/scrybe-app
    mkdir -p ~/Applications
    cp -R {{cargo_target_dir}}/release/bundle/macos/Scrybe.app ~/Applications/

# Alias for people looking for the app-specific install recipe.
app-install: install-app

# Install the Python toolkit entry points the app shells out to at runtime.
install-python-toolkit:
    mkdir -p ~/venv/bin
    rm -f ~/venv/bin/scrybe ~/venv/bin/scrybe-mcp-server ~/venv/bin/scrybe-docx
    cd scrybe-py && VIRTUAL_ENV="$HOME/venv" ~/venv/bin/maturin develop --release
    cd scrybe-mermaid && VIRTUAL_ENV="$HOME/venv" ~/venv/bin/maturin develop --release
    cd scrybe-mcp-server && VIRTUAL_ENV="$HOME/venv" ~/venv/bin/maturin develop --release
    cd scrybe-cli && VIRTUAL_ENV="$HOME/venv" ~/venv/bin/maturin develop --release
    cd scrybe-plugin-docx && ~/venv/bin/python -m pip install -e .
    cd scrybe-meta && ~/venv/bin/python -m pip install -e .

# Install all Python packages in editable/dev mode (compiles Rust binaries)
editable:
    cd scrybe-py && maturin develop --release
    cd scrybe-mermaid && maturin develop --release
    cd scrybe-mcp-server && maturin develop --release
    cd scrybe-cli && maturin develop --release
    cd scrybe-plugin-docx && python -m pip install -e .
    cd scrybe-meta && python -m pip install -e .

# Build the Tauri desktop app (requires npm install first)
app:
    cd scrybe-app && npm install && npm run tauri build

# Run the Tauri app in development mode
dev:
    cd scrybe-app && npm install && npm run tauri dev
