# Legacy configuration import

The desktop app imports data from an older project-local `config/` directory only when the process is started with `SELF_TOOLS_LEGACY_CONFIG` set to that directory. It does not search the current working directory or inspect the default Tauri config directory for possible legacy data.

## Explicit import

Set `SELF_TOOLS_LEGACY_CONFIG` to the old directory. Set `SELF_TOOLS_HOME` to the new data root when you want to choose the destination; the importer writes into its `config/` child. Close other Self-Tools instances before starting the import.

macOS example:

```sh
SELF_TOOLS_LEGACY_CONFIG="/absolute/path/to/old/config" \
SELF_TOOLS_HOME="$HOME/Library/Application Support/Self-Tools" \
"/Applications/DevToolbox.app/Contents/MacOS/DevToolbox"
```

Windows PowerShell example:

```powershell
$env:SELF_TOOLS_LEGACY_CONFIG = 'C:\absolute\path\to\old\config'
$env:SELF_TOOLS_HOME = "$env:LOCALAPPDATA\Self-Tools"
& '.\DevToolbox.exe'
```

Without `SELF_TOOLS_HOME`, a release build uses the platform's Tauri application config directory. A debug build uses the project-local `config/` directory when it can locate the project root, otherwise it uses the Tauri application config directory. The source must be a different, existing directory from the destination; overlapping roots are rejected.

## Imported files

The importer considers these app-owned SQLite databases, if present:

- `dashboard.db`, `news.db`, `travel.db`, `language.db`, `geography.db`, `learning.db`
- `conversations.db`, `study_boards.db`, `history_enrichment.db`
- `memory.db`, `documents.db`, `files.db`, `server_actions.db`

It also imports regular files under `language/nce/`. SQLite databases are copied using a consistent SQLite snapshot, checked with `PRAGMA integrity_check`, and installed without overwriting a destination file. Directory links, file links, corrupt databases, path traversal and overlapping roots are rejected. Existing destination files are preserved. The source is never removed or altered by this importer.

`settings.json` is intentionally excluded because it may contain provider credentials. Configure settings and secrets again in the new installation. Arbitrary `file_roots` and `document_roots`, caches, downloaded corpora, and the immutable bundled History database are also excluded.

A source-path-specific completion marker makes a successful import idempotent. If an import stops before the marker is written, starting again retries missing items while retaining destination files that already exist. A successful empty-source import is also recorded; confirm the source path before starting.

## Verification boundary

The migration tests use disposable SQLite databases and temporary directories. They cover WAL snapshots, existing destination preservation, interrupted retries, corrupt input, empty input, media, symlinks, overlapping roots and regular-file validation. They do not inspect or migrate any existing user data. After a real import, confirm the expected records through the app and keep the old directory until that review is complete.
