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
    rm -f scrybe-app/src-tauri/scrybe-*

# Full install: app + Python toolkit + an idempotent ~/.local/bin/scrybe link.
install: install-app

# Install the desktop app plus its runtime Python tools.
install-app: app install-python-toolkit
    rm -rf ~/Applications/Scrybe.app
    rm -f ~/venv/bin/scrybe ~/venv/bin/scrybe-app
    mkdir -p ~/venv/bin
    cp {{cargo_target_dir}}/release/bundle/macos/Scrybe.app/Contents/MacOS/scrybe-app ~/venv/bin/scrybe-app
    mkdir -p ~/Applications
    cp -R {{cargo_target_dir}}/release/bundle/macos/Scrybe.app ~/Applications/
    ~/Applications/Scrybe.app/Contents/MacOS/scrybe shell-command install

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
    cd scrybe-app && npm install && npm run tauri -- build --config src-tauri/tauri.bundle.conf.json

# Run the Tauri app in development mode
dev:
    cargo build -p scrybe-cli
    cd scrybe-app && npm install && npm run tauri dev

# Re-run the complete idempotent installer. This repairs missing app files and
# broken/older managed command links without overwriting unrelated commands.
repair-install: install

# Remove only development-install artifacts owned by this recipe. User
# documents/configuration and package-manager installations remain untouched.
uninstall:
    cargo run --quiet -p scrybe-cli -- shell-command uninstall
    rm -rf ~/Applications/Scrybe.app
    rm -f ~/venv/bin/scrybe-app ~/venv/bin/scrybe ~/venv/bin/scrybe-mcp-server ~/venv/bin/scrybe-docx
    ~/venv/bin/python -m pip uninstall -y scrybe.ai scrybe-cli scrybe-mcp-server scrybe-py scrybe-mermaid scrybe-plugin-docx
