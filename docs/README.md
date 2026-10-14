# Keel documentation

Guides and references for working with Keel workspaces. Everything here describes the on-disk format and the runtime as implemented in `keel-engine` — the same crate the desktop app and `keel-cli` are built on.

| Doc | Contents |
| --- | --- |
| [Workspace layout](workspace.md) | What a Keel workspace looks like on disk, what is committed, what stays local |
| [Format reference](format.md) | Every field of every file: `collection.yaml`, `folder.yaml`, requests, environments, flows |
| [Variables and environments](variables.md) | `{{name}}` scopes and precedence, defaults vs current values, secrets, builtins, response chaining |
| [Scripting](scripts.md) | Pre-request and post-response scripts: `keel`, `req`, `res`, `test()`, `expect()`, `pm.*` |
| [Tests tab](tests.md) | Declarative assertions: `expect: response.status` plus one matcher |
| [Runs and flows](runs-and-flows.md) | Collection runs, data files, saved flows |
| [CLI](cli.md) | `keel-cli` command reference |
| [Import and export](import-export.md) | cURL, OpenAPI 3, Postman v2.1 import; OpenAPI export; code generation |
| [Using keel-engine](using-keel-engine.md) | Driving workspaces from your own CLI, editor plugin or CI job |

Quick start: clone the repo, `npm install && npm run install:all && npm run dev`, then open `examples/demo-workspace` — it contains working examples of everything below.
