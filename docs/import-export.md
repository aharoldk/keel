# Import and export

## Import

| Source | Where |
| --- | --- |
| cURL | GUI paste dialog or `keel import curl "<text>" --folder F` |
| OpenAPI 3 | GUI import or `keel import openapi ./spec.yaml --folder F` |
| Postman v2.1 | GUI import or `keel import postman ./collection.json --folder F` |

All three use the same importers in the GUI and the CLI, so an import produces identical files either way. Imported requests land in the folder you choose — one YAML file per request — and inherit whatever that folder declares.

## Export

- **OpenAPI 3.0** — `keel export openapi [-o FILE]` (also from the GUI). Built from the request files: methods, URLs, params, headers and bodies become an OpenAPI document.
- **Code generation** — from the GUI, generate client code for the current request as curl, fetch, axios, Python, HTTPie, Go, Java or Node.
