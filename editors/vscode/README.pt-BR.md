# Truxt para VS Code (extensão local inicial)

[English](README.md) · [Contratos do tooling](../../docs/project-analysis-and-lsp.pt-BR.md)

Sintaxe, brackets/comentários, diagnósticos frontend/lint de arquivos não salvos e
tasks check/build/lint/run. Requer VS Code 1.85+ e executável Truxt com `lsp`.
Tasks build/check/run precisam de Cargo.

Na raiz do repositório:

```sh
cargo build -p truxt_cli --locked
cd editors/vscode
npm ci --ignore-scripts
```

Abra `editors/vscode` no VS Code e pressione F5 para iniciar o Extension Development
Host. Abra seu projeto Truxt nesse host. Alternativamente:
`code --extensionDevelopmentPath="<checkout absoluto>/editors/vscode" "<projeto>"`.
Configure o executável nos settings desse host; exemplo Windows:

```json
{
  "truxt.serverPath": "H:\\projetos\\Trusk\\target\\debug\\truxt.exe",
  "truxt.entry": "examples/modules/main.truxt"
}
```

Com entry vazia, cada fonte seleciona seu projeto ancestral `truxt.toml` ou entrada
legada; fonte fora do grafo declarado recebe erro explícito. Configure entry para
selecionar um grafo; outros arquivos ficam fora desse escopo. Manifestos salvos
são monitorados; buffers TOML não salvos não são sincronizados.
Entradas relativas usam a primeira pasta do workspace. Mudanças reiniciam o servidor;
**Truxt: Restart Language Server** reinicia manualmente. O canal **Truxt** contém
logs. Somente documentos locais são suportados; não há servidores separados por
projeto em workspaces múltiplos.

Use **Tasks: Run Task** para a entrada configurada ou arquivo Truxt ativo. Tasks
leem arquivos salvos. Exemplo persistente `.vscode/tasks.json`:

```json
{
  "version": "2.0.0",
  "tasks": [{ "type": "truxt", "command": "check", "file": "main.truxt", "label": "Check Truxt" }]
}
```

`npm run check` valida sintaxe JavaScript. Testes Rust cobrem protocolo LSP,
overlays do grafo, versões e UTF-16. O smoke test visual no host da extensão ainda
é manual. É fonte local, sem publicação Marketplace. Completion, navegação,
formatação, fixes e debugging continuam futuros.
