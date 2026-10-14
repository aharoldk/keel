# Workspace layout

A workspace is a folder of YAML files in a Git repository. The folder is the project; Git is how it is shared. There is no account and no cloud workspace.

```text
collection.yaml          collection root: name, variables, default auth/headers/scripts
folder.yaml              folder metadata: inherited by every request below it
**/*.yaml                one request per file
environments/*.yaml      environments: variables and declared secrets
flows/**/*.yaml          saved request sequences
.keel/workspace.yaml     local workspace variables (never committed)
.keel/env-values.yaml    local "current value" overrides (never committed)
.keel/history.jsonl      append-only send history (never committed)
```

## File rules

- **`collection.yaml`** sits at the workspace root. It names the collection and can carry default variables, auth, headers and scripts for every request inside it. Without it, a folder is not discovered as a workspace — the CLI walks up looking for this file.
- **`folder.yaml`** is optional in any subdirectory. Auth, headers, scripts and variables declared there are inherited by every request below the folder — the nearest folder wins. The directory name is the folder's identity; `name` is only a display label.
- **Request files** are any `*.yaml` that is not one of the reserved names. One request per file. The file name is the stable identity — renaming a request renames the file, and flows reference requests by workspace-relative path.
- **Environments** live in `environments/`. Each file defines variables and declares secrets. See [Variables and environments](variables.md).
- **Flows** live in `flows/`. See [Runs and flows](runs-and-flows.md).
- The runner ignores `.git`, `.keel`, `environments`, `node_modules` and `target` when collecting requests.

## What is committed, what stays local

Committed (part of the shared project):

- `collection.yaml`, `folder.yaml`, all request files, `environments/*.yaml`, `flows/*.yaml`
- Each variable's **default value**, declared next to the variable in its environment, collection, folder or request file
- Secret **names** and their committed default fallbacks, declared in `environments/*.yaml`

Never committed (created by the app on first use):

- `.keel/workspace.yaml` — workspace-scoped variables
- `.keel/env-values.yaml` — current values that override committed defaults
- `.keel/history.jsonl` — history; URLs are stored unresolved (as templates) so secrets never leak into history

Secret current values are not written to either file — they live in the OS keychain.

The recommended `.gitignore` for a Keel workspace:

```gitignore
.keel/
```

## Editing outside the app

Keel watches the workspace with a filesystem watcher, so edits made in your editor show up without a reload. The Git panel covers status, staging, commit, log, per-file diffs, branches, pull and push. Anything that can be done from the terminal can be done in the app, and anything done from the terminal is picked up.
