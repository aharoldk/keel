# CLI

`keel-cli` runs Keel workspaces without the GUI — the same engine, offline. While developing, the binary is `target/debug/keel-cli`; release builds via `cargo build --release -p keel --bin keel-cli --features cli`.

## Commands

```text
keel <COMMAND> [OPTIONS]

list [--folder F] [--env E]                print the collection tree
run <paths...> [--env E] [--bail]          run requests (files) or folders
    [--delay MS] [--data FILE] [--tests-only]
    [--output FILE] [--format json]
import curl "<text>" --folder F            create a request from curl
import openapi <file> [--folder F]         import an OpenAPI 3 document
import postman <file> [--folder F]         import a Postman v2.1 collection
export openapi [--folder F] [-o FILE]      write an OpenAPI 3.0 document
-V, --version                              print version
-h, --help                                 print help
```

## list

Prints the workspace tree with method, path and name per request:

```bash
keel list
keel list --folder users --env local
```

## run

```bash
keel run auth/login.yaml --env local
keel run users/ --bail
keel run users/ --delay 100 --output report.json --format json
keel run users/ --data rows.csv
keel run users/ --tests-only
```

- Paths are request files or folders, relative to the current directory or absolute.
- `--env` accepts the environment file name, file stem or display name.
- `-r` is accepted for compatibility; folders recurse by default.
- `--tests-only` skips requests without tests in the tally (they count as skipped).
- `--output` writes the JSON report described in [Runs and flows](runs-and-flows.md#reports).

Example output:

```text
PASS  GET   {{baseUrl}}/api/users  200  43ms
PASS  POST  {{baseUrl}}/api/users  201  51ms  1/1 tests
Total 2 · Passed 2 · Failed 0 · Errored 0 · Skipped 0
```

The CLI never prints resolved URLs (the template only) or secret values.

## import

```bash
keel import curl "curl https://api.x/y -H 'a: b'" --folder misc
keel import openapi ./spec.yaml --folder api
keel import postman ./collection.json --folder api
```

Same importers as the GUI: cURL, OpenAPI 3 and Postman v2.1. See [Import and export](import-export.md).

## export

```bash
keel export openapi -o openapi.yaml
keel export openapi --folder users
```

Writes an OpenAPI 3.0 document built from the request files. Without `-o` it prints to stdout.

## Workspace discovery

The CLI walks up from the working directory looking for `collection.yaml`. If none is found, it tries the parents of the path arguments. Errors and unknown flags exit with code `2`.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | All tests passed |
| `1` | A failed or errored request |
| `2` | Invalid arguments or paths |
