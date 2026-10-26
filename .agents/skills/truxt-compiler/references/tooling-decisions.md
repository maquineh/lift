# Incremento: API de projeto, lint e LSP inicial

Contratos bilíngues: `docs/project-analysis-and-lsp.md` / `docs/project-analysis-and-lsp.pt-BR.md`.

- `lift_project` extrai loader/SourceMap antes privados da CLI. `ProjectSession`
  mantém overlays; `analyze` retorna `Analysis { entry, sources, hir, diagnostics }`.
  Falhas preservam fontes/spans sem devolver HIR parcial.
- Overlays preservam grafia absoluta estável, inclusive `..`; aliases são resolvidos
  pelo filesystem por análise, sem colapso lexical através de symlinks.
  URIs simultâneas usam ordem global didOpen/didChange: o buffer/URI/versão mais
  recente é autoritativo; demais aliases são limpos e close restaura o restante.
  Aliases canônicos são resolvidos
  por análise (última atualização vence), inclusive após salvar sob junction.
  Documentos LSP usam identidade URI estável. Posições reconhecem LF/CRLF/CR.
  JSON inválido/envelope inválido recebe -32700/-32600 sem perder a sessão.
  Códigos sema têm mapeamento explícito, independente de Debug do enum.
- Assinaturas prévias de lexer/parser/sema/codegen permanecem; o incremento
  Forklift posterior adiciona variantes AST e entrada sema contextual separada.
  SourceId pertence a um resultado; offsets permanecem bytes UTF-8 semiabertos.
  Erros I/O da entrada não têm span; I/O importado aponta ao importador.
- Build/check/run usa a API antes de emitir Rust/Cargo. Sessão/lint/LSP não geram
  arquivos nem validam imports com rustc. Cada análise relê o grafo; não há cache
  incremental de análise. Manifestos locais agora seguem `forklift-decisions.md`.
- Lint HIR advisory: unused-binding (IDs, escrita conta, captura visitada,
  `_`/this isentos), literal-condition e discarded-value limitado. Warning sai 0;
  `--deny-warnings` sai 1; erros fonte/sema continuam Error/1. Sem autofix.
- `lift lsp`: JSON-RPC stdio, full-sync, overlays multi-file, UTF-16, versões,
  relatedInformation e limpeza de diagnósticos. Análise síncrona; versões antigas
  ignoradas. Entry opcional determina grafo; sem ela cada fonte escolhe projeto
  manifestado ancestral ou root legado; fora do grafo gera diagnóstico explícito.
  Watcher relê disco; close remove overlay. Sem features de edição ou DAP.
- `editors/vscode`: cliente JS com vscode-languageclient 9.0.1, grammar,
  language configuration e tasks ProcessExecution (arquivo salvo). Entry relativo
  usa primeira pasta. Fonte local, sem publicação Marketplace.
- API/protocolo/CLI têm testes; smoke visual VSCode segue manual. Forklift por
  path está implementado; bindings, callbacks/async/lift_web e DAP seguem próximos.
  Não escolher driver sem validar PostgreSQL 8.4/9.1; podem requerer legado.
