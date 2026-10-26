# Truxt for VS Code (initial local extension)

[Português do Brasil](README.pt-BR.md) · [Tooling contracts](../../docs/project-analysis-and-lsp.md)

Syntax, brackets/comments, frontend/lint diagnostics for unsaved files, and
check/build/lint/run tasks. Requires VS Code 1.85+ and a Truxt executable with
`lsp`. Cargo is needed for build/check/run tasks.

From the repository root:

```sh
cargo build -p truxt_cli --locked
cd editors/vscode
npm ci --ignore-scripts
```

Open `editors/vscode` in VS Code and press F5 to launch an Extension Development
Host. Open your Truxt project in that host. Alternatively:
`code --extensionDevelopmentPath="<absolute checkout>/editors/vscode" "<project>"`.
Set the executable in that host's settings; Windows example:

```json
{
  "truxt.serverPath": "H:\\projetos\\Trusk\\target\\debug\\truxt.exe",
  "truxt.entry": "examples/modules/main.truxt"
}
```

With entry empty, each open source selects its ancestral `truxt.toml` project or
a legacy entry; a source outside the declared graph receives an explicit error.
Configure entry to select one graph; unrelated open files are outside its scope.
Saved manifest changes are watched; unsaved TOML buffers are not synchronized.
Relative entries use the first workspace folder. Changes restart the server;
**Truxt: Restart Language Server** restarts manually. The **Truxt** output channel
contains server logs. Only local file documents are supported; multiple projects
are not routed to separate servers.

Use **Tasks: Run Task** for the configured entry or active Truxt file. Tasks read
saved disk files. Persistent `.vscode/tasks.json` example:

```json
{
  "version": "2.0.0",
  "tasks": [{ "type": "truxt", "command": "check", "file": "main.truxt", "label": "Check Truxt" }]
}
```

`npm run check` checks client JavaScript syntax. Rust tests cover LSP protocol,
graph overlays, versions and UTF-16. A live extension-host UI smoke test is still
manual. This is local source, not a Marketplace publication. Completion,
navigation, formatting, fixes and debugging remain future work.
