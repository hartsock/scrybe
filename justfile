# Scrybe development tasks

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
install: app
    rm -f ~/venv/bin/scrybe ~/venv/bin/scrybe-app ~/venv/bin/scrybe-mcp-server
    rm -rf ~/Applications/Scrybe.app
    cp target/release/bundle/macos/Scrybe.app/Contents/MacOS/scrybe-app ~/venv/bin/scrybe-app
    mkdir -p ~/Applications
    cp -R target/release/bundle/macos/Scrybe.app ~/Applications/
    cd scrybe-mcp-server && ~/venv/bin/maturin develop --release
    cd scrybe-cli && ~/venv/bin/maturin develop --release

# Install all Python packages in editable/dev mode (compiles Rust binaries)
editable:
    pip install -e .
    cd scrybe-mcp-server && maturin develop --release
    cd scrybe-cli && maturin develop --release

# Build the Tauri desktop app (requires npm install first)
app:
    cd scrybe-app && npm install && npm run tauri build

# Run the Tauri app in development mode
dev:
    cd scrybe-app && npm install && npm run tauri dev
