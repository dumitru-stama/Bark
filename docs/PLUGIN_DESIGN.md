# Bark Plugin System - Design Document

This document describes the Bark plugin architecture in detail. It covers the
communication protocol, the three plugin types, and provides complete examples
for building plugins in any language.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Plugin Discovery and Loading](#plugin-discovery-and-loading)
3. [Communication Protocol](#communication-protocol)
4. [Plugin Types](#plugin-types)
   - [Provider Plugins](#provider-plugins)
   - [Viewer Plugins](#viewer-plugins)
   - [Status Bar Plugins](#status-bar-plugins)
5. [Data Types](#data-types)
6. [Error Handling](#error-handling)
7. [Writing a Provider Plugin](#writing-a-provider-plugin)
8. [Writing a Viewer Plugin](#writing-a-viewer-plugin)
9. [Writing a Status Bar Plugin](#writing-a-status-bar-plugin)
10. [Writing Plugins in Rust](#writing-plugins-in-rust)
11. [Writing Plugins in Python](#writing-plugins-in-python)
12. [Writing Plugins in Other Languages](#writing-plugins-in-other-languages)
13. [Installation and Deployment](#installation-and-deployment)
14. [Debugging Plugins](#debugging-plugins)
15. [Reference: Complete Command Catalog](#reference-complete-command-catalog)

---

## Architecture Overview

Bark plugins are **standalone executables** that communicate with the host
application via **JSON over stdin/stdout**. This design provides:

- **Language independence** -- plugins can be written in any language (Rust,
  Python, Go, C, shell scripts, etc.)
- **Crash isolation** -- a buggy plugin cannot crash the main Bark process
- **No ABI coupling** -- no need to match compiler versions, Rust editions, or
  library versions
- **Simple deployment** -- drop an executable into the plugin directory

There are three plugin types:

| Type | Purpose | Examples |
|------|---------|---------|
| **Provider** | Remote/virtual filesystem access | FTP, S3, Google Drive, archives |
| **Viewer** | Custom file viewers | ELF inspector, image preview, PDF viewer |
| **Status** | Status bar extensions | System memory, git info, disk usage |

### Execution Model

Provider plugins run as **persistent child processes**. When the user opens a
connection, Bark spawns the plugin executable and keeps it alive for the
duration of the session. Commands are sent line-by-line over stdin, and
responses are read line-by-line from stdout. This allows the plugin to maintain
state (open connections, cached data, etc.) across multiple operations.

Viewer and status plugins run as **short-lived processes**. Each command spawns
a fresh instance of the executable, sends the request on stdin, reads the
response from stdout, and the process exits. This keeps things simple for
plugins that don't need persistent state.

### Diagram

```
┌──────────────────────────────────────────────────┐
│                    Bark (host)                    │
│                                                   │
│  PluginManager                                    │
│  ├── load_from_directory(~/.config/bark/plugins/) │
│  │   ├── bark-ftp --plugin-info → "provider"      │
│  │   ├── bark-elf-viewer --plugin-info → "viewer"  │
│  │   └── system_status.py --plugin-info → "status" │
│  │                                                │
│  ├── Provider plugins (persistent process)        │
│  │   stdin →  {"command":"list_directory",...}     │
│  │   stdout ← {"entries":[...]}                   │
│  │                                                │
│  ├── Viewer plugins (one-shot process)            │
│  │   stdin →  {"command":"viewer_render",...}      │
│  │   stdout ← {"lines":[...],"total_lines":N}    │
│  │                                                │
│  └── Status plugins (one-shot process)            │
│      stdin →  {"command":"status_render",...}      │
│      stdout ← {"text":"Mem: 4.2/16GB"}           │
└──────────────────────────────────────────────────┘
```

---

## Plugin Discovery and Loading

### Discovery Process

On startup, Bark scans `~/.config/bark/plugins/` for executable files. For
each candidate, it runs:

```
./plugin-executable --plugin-info
```

The plugin must print a single line of JSON to stdout and exit with code 0.
Bark parses the `"type"` field to determine the plugin category and loads it
accordingly.

### What Counts as Executable

| Platform | Recognized as executable |
|----------|------------------------|
| **Linux/macOS** | Files with the execute bit set (`chmod +x`), or files with extensions: `.py`, `.rb`, `.pl`, `.sh`, `.bash` |
| **Windows** | Files with extensions: `.exe`, `.py`, `.bat`, `.cmd`, `.ps1`, `.rb`, `.pl` |

### Plugin Directory

The default plugin directory is:

```
~/.config/bark/plugins/
```

You can install plugins with:

```bash
cp my-plugin ~/.config/bark/plugins/
chmod +x ~/.config/bark/plugins/my-plugin
```

For Rust plugins that are part of the Bark workspace, `make install-plugins`
copies the compiled binaries automatically.

---

## Communication Protocol

All communication uses single-line JSON messages. Each request is one line on
stdin, each response is one line on stdout. No multi-line JSON, no streaming,
no framing -- just `\n`-delimited JSON.

### Request Format

Every request is a JSON object with a `"command"` field:

```json
{"command":"command_name","param1":"value1","param2":"value2"}
```

### Response Format

Responses are JSON objects. The structure depends on the command. Errors use
the `"error"` field:

```json
{"error":"Something went wrong","error_type":"not_found"}
```

Success responses vary by command (see the command catalog below).

### Binary Data

Binary file contents are encoded as **base64** strings in the JSON. The plugin
is responsible for encoding (when returning file data) and decoding (when
receiving file data to write).

### String Escaping

All string values must be properly JSON-escaped. At minimum, escape these
characters:

| Character | Escape |
|-----------|--------|
| `\` | `\\` |
| `"` | `\"` |
| newline | `\n` |
| carriage return | `\r` |
| tab | `\t` |

---

## Plugin Types

### Provider Plugins

Provider plugins expose a virtual or remote filesystem. They appear in the
Alt+F1/F2 source selector and let users browse, read, write, and manage files
on remote systems or inside container formats.

Provider plugins declare one or both of:

- **`schemes`** -- URI schemes they handle (e.g., `["ftp", "ftps"]`). These
  appear in the source selector with a connection dialog.
- **`extensions`** -- File extensions they handle (e.g., `["zip", "tar.gz"]`).
  These activate automatically when the user enters a matching file.

**Lifecycle:**

1. User selects the provider from the source selector (or enters a matching
   file)
2. Bark shows a connection dialog using the fields from `get_dialog_fields`
3. User fills in the form, Bark calls `validate_config`
4. Bark spawns a persistent child process and sends `connect`
5. The plugin connects to the remote service and returns `session_id`
6. Bark sends filesystem commands (`list_directory`, `read_file`, etc.) over
   the session
7. When the user disconnects, Bark sends `disconnect` and kills the process

**Required plugin-info fields:**

```json
{
  "name": "My Provider",
  "version": "1.0.0",
  "type": "provider",
  "description": "Access files on My Service",
  "icon": "🌐",
  "schemes": ["myproto"]
}
```

Or for extension-based providers (like archives):

```json
{
  "name": "My Archive Format",
  "version": "1.0.0",
  "type": "provider",
  "description": "Browse .xyz archives",
  "extensions": ["xyz", "xyz2"],
  "icon": "📦"
}
```

**Commands:**

| Command | Purpose |
|---------|---------|
| `get_dialog_fields` | Return connection dialog field definitions |
| `validate_config` | Validate user-provided configuration |
| `connect` | Establish a session |
| `disconnect` | Clean up and close session |
| `list_directory` | List files in a directory |
| `read_file` | Read file contents (base64) |
| `write_file` | Write file contents (base64) |
| `delete` | Delete a file or directory |
| `mkdir` | Create a directory |
| `rename` | Rename/move a file or directory |
| `copy_file` | Copy a file within the provider |
| `set_attributes` | Set modification time and permissions on a file |

### Viewer Plugins

Viewer plugins provide custom file viewing. When the user presses F3 (View),
Bark checks all viewer plugins to find one that can handle the file. The plugin
with the highest priority wins.

**Lifecycle:**

1. User presses F3 on a file
2. Bark sends `viewer_can_handle` to each viewer plugin with the file path
3. Plugins that recognize the file return `{"can_handle": true, "priority": N}`
4. Bark picks the plugin with the highest priority
5. Bark sends `viewer_render` with path, dimensions, and scroll offset
6. The plugin returns rendered text lines
7. As the user scrolls, Bark sends updated `viewer_render` requests

**Required plugin-info fields:**

```json
{
  "name": "My Viewer",
  "version": "1.0.0",
  "type": "viewer",
  "description": "View .xyz files",
  "icon": "🔍",
  "extensions": ["xyz", "xyz2"]
}
```

The `extensions` field is informational (for listing purposes). The actual
matching happens via `viewer_can_handle`, which can inspect file contents
(magic bytes) for more accurate detection.

**Commands:**

| Command | Purpose |
|---------|---------|
| `viewer_can_handle` | Check if plugin can display this file |
| `viewer_render` | Render visible lines for the viewer |

### Status Bar Plugins

Status bar plugins display information in Bark's bottom status line. They
receive context about the current panel state and return a short text string.

**Lifecycle:**

1. Bark periodically calls `status_render` with current panel context
2. The plugin returns a short text string
3. Bark displays it in the status bar

**Required plugin-info fields:**

```json
{
  "name": "My Status",
  "version": "1.0.0",
  "type": "status",
  "description": "Shows custom info in status bar",
  "icon": "📊"
}
```

**Commands:**

| Command | Purpose |
|---------|---------|
| `status_render` | Return text for the status bar |

---

## Data Types

### FileEntry

Returned by `list_directory` in the `entries` array. Each entry represents a
file or directory.

```json
{
  "name": "document.txt",
  "path": "/remote/path/document.txt",
  "is_dir": false,
  "size": 45678,
  "modified": 1706400000,
  "is_hidden": false,
  "permissions": 644,
  "is_symlink": false,
  "symlink_target": null,
  "owner": "user",
  "group": "staff"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | File/directory name (not full path) |
| `path` | string | no | Full path within the provider namespace. If omitted, Bark constructs it from the parent path + name. |
| `is_dir` | bool | yes | Whether this is a directory |
| `size` | int | yes | File size in bytes (0 for directories) |
| `modified` | int | no | Unix timestamp (seconds since epoch) |
| `is_hidden` | bool | no | Whether the file is hidden (defaults to `name.startswith(".")`) |
| `permissions` | int | no | Unix permission bits (e.g., 755). 0 if not available. |
| `is_symlink` | bool | no | Whether this is a symbolic link |
| `symlink_target` | string | no | Symlink target path, if applicable |
| `owner` | string | no | Owner user name |
| `group` | string | no | Owner group name |

### DialogField

Returned by `get_dialog_fields` in the `fields` array. Each field defines one
input in the connection dialog.

```json
{
  "id": "host",
  "label": "Hostname",
  "type": "text",
  "required": true,
  "default": "example.com",
  "placeholder": "server.example.com",
  "help": "The server hostname or IP address"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Field identifier (used as config key) |
| `label` | string | yes | Display label in the dialog |
| `type` | string | yes | Field type (see below) |
| `required` | bool | no | Whether the field must be filled in (default: false) |
| `default` | string | no | Default value |
| `placeholder` | string | no | Placeholder text shown when empty |
| `help` | string | no | Help text shown below the field |

**Field types:**

| Type | Description | Example use |
|------|-------------|-------------|
| `text` | Single-line text input | Hostname, username, bucket name |
| `password` | Masked text input | Password, API key |
| `number` | Numeric input | Port number |
| `checkbox` | Boolean toggle | "Use TLS", "Passive mode" |
| `select` | Dropdown selection | Protocol version, region |
| `textarea` | Multi-line text input | SSH key, notes |
| `filepath` | File path selector | Key file path |

For `select` fields, include an `options` array:

```json
{
  "id": "protocol",
  "label": "Protocol",
  "type": "select",
  "options": [
    {"value": "ftp", "label": "FTP (plain)"},
    {"value": "ftps", "label": "FTPS (TLS)"}
  ]
}
```

### ProviderConfig

Sent to `validate_config` and `connect`. Contains the user's filled-in dialog
values as a flat key-value object:

```json
{
  "name": "My Server",
  "host": "ftp.example.com",
  "port": "21",
  "user": "admin",
  "password": "secret",
  "path": "/data",
  "use_tls": "true"
}
```

All values are strings. Booleans are `"true"` or `"false"`. Numbers are string
representations (`"21"` not `21`). The `name` field is the connection name the
user gave in the dialog.

---

## Error Handling

### Error Response Format

When a command fails, return a JSON object with `"error"` and optionally
`"error_type"`:

```json
{"error": "Connection refused", "error_type": "connection"}
```

### Error Types

| `error_type` | Meaning |
|-------------|---------|
| `connection` | Connection failed (network error, timeout) |
| `auth` | Authentication failed (bad credentials) |
| `not_found` | File or directory not found |
| `permission` | Permission denied |
| `config` | Configuration error (missing required fields) |
| *(omitted)* | Generic error |

If `error_type` is omitted, the error is treated as a generic error.

### Non-Error Failures

For commands that return `"success"`, a failure looks like:

```json
{"success": false, "error": "Disk full"}
```

For `validate_config`:

```json
{"valid": false, "error": "Host is required"}
```

---

## Writing a Provider Plugin

This section walks through creating a complete provider plugin.

### Step 1: Handle `--plugin-info`

When invoked with `--plugin-info` as the first command-line argument, print
plugin metadata as JSON and exit:

```
$ ./my-provider --plugin-info
{"name":"My Provider","version":"1.0.0","type":"provider","schemes":["myproto"],"description":"Access My Service files","icon":"🌐"}
```

### Step 2: Read Commands from stdin

When invoked without `--plugin-info`, enter a read loop on stdin. Each line is
a JSON command. Process it and write one line of JSON response to stdout.

**Important:** Flush stdout after every response line. Bark reads one line and
blocks until it arrives. If you forget to flush, the host will hang.

```
stdin:  {"command":"connect","config":{"host":"example.com","user":"admin","password":"pass123"}}
stdout: {"success":true,"session_id":"abc123"}
stdin:  {"command":"list_directory","session_id":"abc123","path":"/"}
stdout: {"entries":[{"name":"docs","is_dir":true,"size":0},{"name":"readme.txt","is_dir":false,"size":1234}]}
stdin:  {"command":"disconnect","session_id":"abc123"}
stdout: {"success":true}
```

### Step 3: Implement Commands

#### `get_dialog_fields`

Return the connection dialog fields.

Request:
```json
{"command":"get_dialog_fields"}
```

Response:
```json
{
  "fields": [
    {"id":"host","label":"Hostname","type":"text","required":true},
    {"id":"port","label":"Port","type":"number","default":"21"},
    {"id":"user","label":"Username","type":"text","required":true},
    {"id":"password","label":"Password","type":"password"},
    {"id":"path","label":"Initial path","type":"text","default":"/"},
    {"id":"use_tls","label":"Use TLS","type":"checkbox","default":"false"}
  ]
}
```

#### `validate_config`

Check if the configuration is valid before attempting to connect.

Request:
```json
{"command":"validate_config","config":{"host":"","user":"admin"}}
```

Response (failure):
```json
{"valid":false,"error":"Host is required"}
```

Response (success):
```json
{"valid":true}
```

#### `connect`

Establish a connection using the provided configuration. Return a session ID
and optionally a short label for the panel header.

Request:
```json
{"command":"connect","config":{"host":"ftp.example.com","port":"21","user":"admin","password":"secret","path":"/data","use_tls":"false"}}
```

Response (success):
```json
{"success":true,"session_id":"session-001","short_label":"[FTP]"}
```

The `short_label` is displayed in the panel header (e.g., `[FTP] /data`). If
omitted, Bark uses the provider name.

Response (failure):
```json
{"success":false,"error":"Connection refused"}
```

#### `list_directory`

List the contents of a directory.

Request:
```json
{"command":"list_directory","session_id":"session-001","path":"/data"}
```

Response:
```json
{
  "entries": [
    {"name":"reports","is_dir":true,"size":0,"permissions":755},
    {"name":"config.json","is_dir":false,"size":2048,"modified":1706400000,"permissions":644}
  ]
}
```

Do **not** include `.` or `..` entries. Bark adds the parent directory entry
(`..`) automatically.

#### `read_file`

Read file contents and return as base64.

Request:
```json
{"command":"read_file","session_id":"session-001","path":"/data/config.json"}
```

Response:
```json
{"data":"eyJrZXkiOiAidmFsdWUifQ=="}
```

#### `write_file`

Write base64-encoded data to a file.

Request:
```json
{"command":"write_file","session_id":"session-001","path":"/data/new.txt","data":"SGVsbG8gV29ybGQ="}
```

Response:
```json
{"success":true}
```

#### `delete`

Delete a file or directory. The `recursive` flag indicates recursive deletion.

Request (single file):
```json
{"command":"delete","session_id":"session-001","path":"/data/old.txt"}
```

Request (recursive directory):
```json
{"command":"delete","session_id":"session-001","path":"/data/old-dir","recursive":true}
```

Response:
```json
{"success":true}
```

#### `mkdir`

Create a directory.

Request:
```json
{"command":"mkdir","session_id":"session-001","path":"/data/new-dir"}
```

Response:
```json
{"success":true}
```

#### `rename`

Rename or move a file/directory.

Request:
```json
{"command":"rename","session_id":"session-001","from":"/data/old.txt","to":"/data/new.txt"}
```

Response:
```json
{"success":true}
```

#### `copy_file`

Copy a file within the provider.

Request:
```json
{"command":"copy","session_id":"session-001","from":"/data/original.txt","to":"/data/copy.txt"}
```

Response:
```json
{"success":true}
```

#### `set_attributes`

Set modification time and/or permissions on a file. This is called after
copying or moving files to preserve attributes from the source. Plugins that
don't support attribute setting can return success without doing anything.

Request:
```json
{"command":"set_attributes","session_id":"session-001","path":"/data/file.txt","modified":1706400000,"permissions":644}
```

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | File path to update |
| `modified` | int or null | Unix timestamp (seconds since epoch), or null to skip |
| `permissions` | int | Unix permission bits (e.g., 644). 0 to skip. |

Response:
```json
{"success":true}
```

This command is best-effort -- errors are silently ignored by Bark.

#### `disconnect`

Clean up resources and close the session. This is the last command before the
process is killed.

Request:
```json
{"command":"disconnect","session_id":"session-001"}
```

Response:
```json
{"success":true}
```

### Read-Only Providers

If your provider is read-only (like an archive browser), return an error for
write operations:

```json
{"error":"Read-only filesystem","error_type":"permission"}
```

---

## Writing a Viewer Plugin

### Step 1: Handle `--plugin-info`

```
$ ./my-viewer --plugin-info
{"name":"My Viewer","version":"1.0.0","type":"viewer","description":"View .xyz files","icon":"🔍","extensions":["xyz","xyz2"]}
```

### Step 2: Implement `viewer_can_handle`

Bark sends the file path. The plugin should check if it can display this file
(by extension, magic bytes, or any other heuristic). Return a priority value --
higher numbers win when multiple viewers match.

Request:
```json
{"command":"viewer_can_handle","path":"/home/user/data.xyz"}
```

Response (can handle):
```json
{"can_handle":true,"priority":10}
```

Response (cannot handle):
```json
{"can_handle":false,"priority":0}
```

**Priority guidelines:**

| Priority | Use case |
|----------|----------|
| 1-5 | Generic/fallback viewers |
| 5-10 | Extension-based viewers |
| 10-20 | Magic-byte-based viewers (more reliable) |
| 20+ | Highly specialized viewers |

### Step 3: Implement `viewer_render`

Bark sends the file path, terminal dimensions, and scroll offset. Return the
visible lines and total line count.

Request:
```json
{"command":"viewer_render","path":"/home/user/data.xyz","width":80,"height":24,"scroll":0}
```

Response:
```json
{
  "lines": [
    "=== XYZ File Header ===",
    "",
    "Format version: 2.1",
    "Created: 2024-01-15",
    "Records: 1,234",
    "",
    "=== Record List ===",
    "1. First record",
    "2. Second record"
  ],
  "total_lines": 150
}
```

The `lines` array contains only the visible portion (starting at `scroll`,
limited to `height` lines). `total_lines` is the total number of lines in the
document, used for scroll bar calculations.

**Tips:**

- Parse the file once, format it into lines, apply `scroll` and `height` to
  slice the visible window
- Keep lines within `width` characters (truncate or wrap as appropriate)
- Use plain text -- Bark does not interpret ANSI escape codes from plugins

---

## Writing a Status Bar Plugin

### Step 1: Handle `--plugin-info`

```
$ ./my-status --plugin-info
{"name":"My Status","version":"1.0.0","type":"status","description":"Shows useful info","icon":"📊"}
```

### Step 2: Implement `status_render`

Request:
```json
{
  "command": "status_render",
  "path": "/home/user/projects",
  "selected_file": "main.rs",
  "is_dir": false,
  "file_size": 4096,
  "selected_count": 3
}
```

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Current directory path |
| `selected_file` | string or null | Currently highlighted filename |
| `is_dir` | bool | Whether the highlighted entry is a directory |
| `file_size` | int | Size of the highlighted file |
| `selected_count` | int | Number of selected (marked) files |

Response:
```json
{"text": "Mem: 8.2/16GB | Load: 1.4"}
```

Keep the text short -- it shares space with other status bar elements. Around
20-40 characters is ideal.

---

## Writing Plugins in Rust

Rust plugins can use the `bark-plugin-api` crate for type definitions, though
it's not required (you can hand-write JSON).

### Cargo.toml

```toml
[package]
name = "bark-my-plugin"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "bark-my-plugin"
path = "src/main.rs"

[dependencies]
bark-plugin-api = { path = "../plugin-api" }
# ... your dependencies
```

### Main Structure

```rust
use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--plugin-info" {
        println!(r#"{{"name":"My Plugin","version":"1.0.0","type":"provider","schemes":["myproto"],"description":"My custom provider","icon":"🔌"}}"#);
        return;
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let response = handle_command(&line);
        writeln!(stdout, "{}", response).ok();
        stdout.flush().ok();  // IMPORTANT: flush after every response
    }
}

fn handle_command(json: &str) -> String {
    let command = extract_string(json, "command").unwrap_or_default();

    match command.as_str() {
        "get_dialog_fields" => handle_get_dialog_fields(),
        "validate_config" => handle_validate_config(json),
        "connect" => handle_connect(json),
        "disconnect" => handle_disconnect(),
        "list_directory" => handle_list_directory(json),
        "read_file" => handle_read_file(json),
        "write_file" => handle_write_file(json),
        "delete" => handle_delete(json),
        "mkdir" => handle_mkdir(json),
        "rename" => handle_rename(json),
        "copy_file" => handle_copy_file(json),
        _ => format!(r#"{{"error":"Unknown command: {}"}}"#, command),
    }
}
```

The existing plugins in `plugins/` are good reference implementations. The FTP
plugin (`plugins/ftp-plugin/`) is a complete provider. The ELF viewer
(`plugins/elf-viewer/`) is a complete viewer.

### JSON Handling

The existing Bark plugins do **not** use `serde_json` or any external JSON
crate. They use hand-rolled JSON parsing to keep binary sizes small and avoid
dependency bloat. You are free to use `serde_json` if you prefer -- Bark
doesn't care how you produce the JSON, only that it's valid.

---

## Writing Plugins in Python

Python is excellent for quick plugins, especially status bar and viewer types.

### Complete Python Provider Plugin

```python
#!/usr/bin/env python3
"""Example provider plugin for Bark."""

import sys
import json
import base64

# -- Plugin state --
session = None

def get_plugin_info():
    return {
        "name": "Example Provider",
        "version": "1.0.0",
        "type": "provider",
        "schemes": ["example"],
        "description": "Example remote filesystem",
        "icon": "🔌"
    }

def get_dialog_fields():
    return {
        "fields": [
            {"id": "host", "label": "Hostname", "type": "text", "required": True},
            {"id": "port", "label": "Port", "type": "number", "default": "8080"},
            {"id": "token", "label": "API Token", "type": "password", "required": True},
        ]
    }

def validate_config(config):
    if not config.get("host"):
        return {"valid": False, "error": "Host is required"}
    if not config.get("token"):
        return {"valid": False, "error": "API token is required"}
    return {"valid": True}

def connect(config):
    global session
    # ... connect to your service using config values ...
    session = {"host": config["host"], "token": config["token"]}
    return {"success": True, "session_id": "sess-001", "short_label": "[EX]"}

def list_directory(path):
    if session is None:
        return {"error": "Not connected"}
    # ... list files at path ...
    return {
        "entries": [
            {"name": "file.txt", "is_dir": False, "size": 1024},
            {"name": "subdir", "is_dir": True, "size": 0},
        ]
    }

def read_file(path):
    if session is None:
        return {"error": "Not connected"}
    # ... read file data ...
    data = b"Hello from the plugin!"
    return {"data": base64.b64encode(data).decode()}

def write_file(path, data_b64):
    if session is None:
        return {"error": "Not connected"}
    data = base64.b64decode(data_b64)
    # ... write data to path ...
    return {"success": True}

def handle_command(request):
    cmd = request.get("command", "")

    if cmd == "get_dialog_fields":
        return get_dialog_fields()
    elif cmd == "validate_config":
        return validate_config(request.get("config", {}))
    elif cmd == "connect":
        return connect(request.get("config", {}))
    elif cmd == "disconnect":
        return {"success": True}
    elif cmd == "list_directory":
        return list_directory(request.get("path", "/"))
    elif cmd == "read_file":
        return read_file(request.get("path", ""))
    elif cmd == "write_file":
        return write_file(request.get("path", ""), request.get("data", ""))
    elif cmd == "delete":
        return {"error": "Read-only", "error_type": "permission"}
    elif cmd == "mkdir":
        return {"error": "Read-only", "error_type": "permission"}
    elif cmd == "rename":
        return {"error": "Read-only", "error_type": "permission"}
    elif cmd == "copy":
        return {"error": "Read-only", "error_type": "permission"}
    else:
        return {"error": f"Unknown command: {cmd}"}

def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--plugin-info":
        print(json.dumps(get_plugin_info()))
        return

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
            response = handle_command(request)
            print(json.dumps(response), flush=True)  # flush is critical
        except json.JSONDecodeError as e:
            print(json.dumps({"error": f"Invalid JSON: {e}"}), flush=True)
        except Exception as e:
            print(json.dumps({"error": str(e)}), flush=True)

if __name__ == "__main__":
    main()
```

### Complete Python Status Plugin

```python
#!/usr/bin/env python3
"""Example status bar plugin."""

import sys
import json
import os

def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--plugin-info":
        print(json.dumps({
            "name": "Disk Usage",
            "version": "1.0.0",
            "type": "status",
            "description": "Shows disk usage for current path",
            "icon": "💾"
        }))
        return

    line = sys.stdin.readline().strip()
    if not line:
        return

    request = json.loads(line)
    if request.get("command") == "status_render":
        path = request.get("path", "/")
        try:
            stat = os.statvfs(path)
            total = stat.f_blocks * stat.f_frsize
            free = stat.f_bavail * stat.f_frsize
            used_pct = int((1 - free / total) * 100) if total > 0 else 0
            total_gb = total / (1024**3)
            free_gb = free / (1024**3)
            print(json.dumps({
                "text": f"Disk: {used_pct}% ({free_gb:.1f}/{total_gb:.1f}GB free)"
            }))
        except:
            print(json.dumps({"text": "Disk: N/A"}))
    else:
        print(json.dumps({"error": "Unknown command"}))

if __name__ == "__main__":
    main()
```

### Complete Python Viewer Plugin

```python
#!/usr/bin/env python3
"""Example viewer plugin for CSV files."""

import sys
import json
import csv
import os

def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--plugin-info":
        print(json.dumps({
            "name": "CSV Viewer",
            "version": "1.0.0",
            "type": "viewer",
            "description": "Tabular CSV file viewer",
            "icon": "📊",
            "extensions": ["csv", "tsv"]
        }))
        return

    line = sys.stdin.readline().strip()
    if not line:
        return

    request = json.loads(line)
    cmd = request.get("command", "")

    if cmd == "viewer_can_handle":
        path = request.get("path", "")
        ext = os.path.splitext(path)[1].lower()
        if ext in (".csv", ".tsv"):
            print(json.dumps({"can_handle": True, "priority": 15}))
        else:
            print(json.dumps({"can_handle": False, "priority": 0}))

    elif cmd == "viewer_render":
        path = request.get("path", "")
        width = request.get("width", 80)
        height = request.get("height", 24)
        scroll = request.get("scroll", 0)

        try:
            with open(path, newline='') as f:
                reader = csv.reader(f)
                all_rows = list(reader)

            # Format as aligned columns
            if not all_rows:
                print(json.dumps({"lines": ["(empty file)"], "total_lines": 1}))
                return

            col_widths = [0] * max(len(r) for r in all_rows)
            for row in all_rows:
                for i, cell in enumerate(row):
                    col_widths[i] = max(col_widths[i], len(cell))

            lines = []
            for row in all_rows:
                cells = [cell.ljust(col_widths[i]) for i, cell in enumerate(row)]
                lines.append(" | ".join(cells)[:width])

            total = len(lines)
            visible = lines[scroll:scroll + height]
            print(json.dumps({"lines": visible, "total_lines": total}))

        except Exception as e:
            print(json.dumps({"error": str(e)}))
    else:
        print(json.dumps({"error": f"Unknown command: {cmd}"}))

if __name__ == "__main__":
    main()
```

---

## Writing Plugins in Other Languages

Any language that can read stdin, write stdout, and produce JSON will work.

### Go

```go
package main

import (
    "bufio"
    "encoding/json"
    "fmt"
    "os"
)

func main() {
    if len(os.Args) > 1 && os.Args[1] == "--plugin-info" {
        fmt.Println(`{"name":"Go Status","version":"1.0","type":"status","description":"Example Go plugin","icon":"🐹"}`)
        return
    }

    scanner := bufio.NewScanner(os.Stdin)
    for scanner.Scan() {
        line := scanner.Text()
        var req map[string]interface{}
        json.Unmarshal([]byte(line), &req)

        if req["command"] == "status_render" {
            resp, _ := json.Marshal(map[string]string{"text": "Go plugin OK"})
            fmt.Println(string(resp))
        }
    }
}
```

### Shell Script

```bash
#!/bin/bash
# Minimal status plugin in bash

if [ "$1" = "--plugin-info" ]; then
    echo '{"name":"Uptime","version":"1.0","type":"status","description":"System uptime","icon":"⏱"}'
    exit 0
fi

read -r line
echo "{\"text\": \"Up: $(uptime -p 2>/dev/null || echo 'N/A')\"}"
```

### Key Rules for Any Language

1. **`--plugin-info`**: When the first CLI argument is `--plugin-info`, print
   one line of JSON metadata and exit with code 0.
2. **stdin/stdout**: Read JSON commands from stdin (one per line), write JSON
   responses to stdout (one per line).
3. **Flush stdout**: After every response line, flush the output buffer. This
   is critical -- Bark blocks waiting for the response.
4. **Exit cleanly**: When stdin closes (EOF), exit gracefully.
5. **stderr is ignored**: Bark redirects stderr to /dev/null. Use stderr for
   debug logging if needed -- it won't interfere with the protocol.

---

## Installation and Deployment

### Manual Installation

```bash
# Copy plugin to the plugin directory
cp my-plugin ~/.config/bark/plugins/

# Make it executable (Linux/macOS)
chmod +x ~/.config/bark/plugins/my-plugin
```

### For Rust Workspace Plugins

Add the plugin to the workspace `Cargo.toml`:

```toml
[workspace]
members = [
    ".",
    "plugins/plugin-api",
    "plugins/my-plugin",
]
```

Then install with:

```bash
make install-plugins
```

### Naming Convention

Plugin executables should be named `bark-<name>`:

- `bark-ftp` -- FTP provider
- `bark-archive` -- Archive provider
- `bark-elf-viewer` -- ELF viewer
- `bark-my-plugin` -- Your plugin

This is a convention, not a requirement. Bark loads any executable from the
plugin directory regardless of its name.

---

## Debugging Plugins

### Test `--plugin-info` Directly

```bash
./my-plugin --plugin-info
```

Should print valid JSON and exit with code 0.

### Test Commands Manually

```bash
echo '{"command":"get_dialog_fields"}' | ./my-plugin
echo '{"command":"viewer_can_handle","path":"/tmp/test.xyz"}' | ./my-plugin
echo '{"command":"status_render","path":"/home/user","selected_file":"test.txt","is_dir":false,"file_size":1024,"selected_count":0}' | ./my-plugin
```

### Test a Full Provider Session

```bash
# Start the plugin and send commands interactively
./my-plugin
{"command":"connect","config":{"host":"example.com","user":"admin","password":"secret"}}
{"command":"list_directory","session_id":"default","path":"/"}
{"command":"read_file","session_id":"default","path":"/test.txt"}
{"command":"disconnect","session_id":"default"}
```

Type each line and check the JSON response.

### Use stderr for Logging

Bark ignores stderr, so you can write debug output there:

```python
import sys
print("DEBUG: processing request", file=sys.stderr)
```

```rust
eprintln!("DEBUG: processing request");
```

### Common Issues

| Problem | Cause | Fix |
|---------|-------|-----|
| Plugin not discovered | Not executable | `chmod +x plugin` |
| Plugin not discovered | `--plugin-info` exits with non-zero | Check for runtime errors (missing libraries, etc.) |
| Plugin hangs | stdout not flushed | Add explicit flush after every `println`/`print` |
| Plugin hangs | Reading past first line in one-shot mode | Status/viewer plugins should read one line and exit |
| Garbled output | Multi-line JSON response | Ensure each response is exactly one line |
| Binary data corrupt | Not using base64 | Encode all binary data as base64 strings |
| "Unknown command" | Typo in command name | Check exact command string (`list_directory` not `ls`) |

---

## Reference: Complete Command Catalog

### Discovery

| Trigger | Behavior |
|---------|----------|
| `./plugin --plugin-info` | Print JSON metadata, exit 0 |

### Provider Commands

| Command | Parameters | Response |
|---------|-----------|----------|
| `get_dialog_fields` | *(none)* | `{"fields": [...]}` |
| `validate_config` | `config: {...}` | `{"valid": true}` or `{"valid": false, "error": "..."}` |
| `connect` | `config: {...}` | `{"success": true, "session_id": "..."}` or `{"success": false, "error": "..."}` |
| `disconnect` | `session_id` | `{"success": true}` |
| `list_directory` | `session_id`, `path` | `{"entries": [...]}` |
| `read_file` | `session_id`, `path` | `{"data": "<base64>"}` |
| `write_file` | `session_id`, `path`, `data` (base64) | `{"success": true}` |
| `delete` | `session_id`, `path`, optional `recursive` | `{"success": true}` |
| `mkdir` | `session_id`, `path` | `{"success": true}` |
| `rename` | `session_id`, `from`, `to` | `{"success": true}` |
| `copy` | `session_id`, `from`, `to` | `{"success": true}` |
| `set_attributes` | `session_id`, `path`, `modified` (int/null), `permissions` (int) | `{"success": true}` |

### Viewer Commands

| Command | Parameters | Response |
|---------|-----------|----------|
| `viewer_can_handle` | `path` | `{"can_handle": bool, "priority": int}` |
| `viewer_render` | `path`, `width`, `height`, `scroll` | `{"lines": [...], "total_lines": int}` |

### Status Commands

| Command | Parameters | Response |
|---------|-----------|----------|
| `status_render` | `path`, `selected_file`, `is_dir`, `file_size`, `selected_count` | `{"text": "..."}` |
