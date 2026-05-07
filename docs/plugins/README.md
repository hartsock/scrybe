# Scrybe Plugins

Plugins are Python scripts in `~/.config/scrybe/plugins/`.

## Protocol

- Receive document source on **stdin**
- Write modified source (or empty) to **stdout**
- Non-zero exit → plugin is skipped with a warning

## Install an example plugin

```bash
mkdir -p ~/.config/scrybe/plugins
cp docs/plugins/example_plugin.py ~/.config/scrybe/plugins/
```

## Python API (scrybe library)

If `pip install scrybe` is installed, plugins can import it:

```python
from scrybe import Document, render_markdown

def on_change(source: str) -> str:
    doc = Document(source)
    # ... transform ...
    return doc.source
```

## ADR

See `docs/adr/0001-python-outside-rust-inside.md` for the architecture rationale.
