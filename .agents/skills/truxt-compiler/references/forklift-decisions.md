# Forklift local v1

Contratos bilíngues: `docs/forklift.md` / `docs/forklift.pt-BR.md`.

- `truxt.toml`: package name/version/kind/entry obrigatórios; dependências apenas
  `{path="..."}` relativas ao manifesto. Identidade é manifesto canônico, não
  alias/nome/versão. Kinds bin/lib; dependências devem ser lib; ciclos de pacote
  rejeitados. Nome/versão são metadados não vazios, sem resolução semver.
- Sintaxe TS-like `import * as alias from "dependency";`, `export function`.
  Namespace de pacote compartilhado pelos módulos flat; repetição alias/target
  idempotente, conflitos rejeitados. Locais/parâmetros sombreiam aliases; alias
  console/option/result é reservado. Exports aceitam primitivos/containers/Result,
  não custom types, closures, interfaces públicas ou reexports.
- `import module` sem manifesto e `analyze(ProgramAst)` mantêm contratos legados.
  No modo manifestado, módulos são flat dentro da raiz sem atravessar outro
  pacote; só entry de bin pode executar statements. Bibliotecas só declaram.
- ProjectAst mantém contexto de pacotes. Sema analisa DAG usando assinaturas
  exportadas com modos finais; namespace resolve antes do receiver nominal com
  precedência de bindings locais. HIR recebe identidades qualificadas e BindingIds
  globais depois de validado, sem renomeação textual da AST ou nova inferência
  no codegen. Liveness/aliasing/clones existentes preservados.
- `ProjectSession::resolve` retorna snapshot/metadados/ProjectAst/SourceMap;
  `Resolution::analyze` consome snapshot sem I/O. Input resolve localiza manifesto;
  entry declarada é autoritativa. `analyze(source)` exige fonte no grafo; fora
  retorna package.outside-graph. Watcher LSP inclui manifesto salvo, sem sync de
  TOML não salvo no cliente inicial.
- Binário `forklift`: init/resolve/check/build/run. Resolve não chama Cargo nem
  verifica tipos; check aceita lib, build/run exigem bin. Reusa snapshot uma vez.
  Init normaliza nome ASCII e usa staging/hardlink sem sobrescrita, com rollback
  em falha de publicação; não é transação indivisível de dois arquivos.
- Pacotes Truxt viram um HIR/programa Cargo; crates Rust FFI continuam dependências
  Cargo. Conflitos de versão FFI entre pacotes são erro sema. Sem registry Truxt,
  rede do resolvedor, lockfile Forklift, remotos ou instalação automática.
- Exemplo `examples/packages/app` e contratos cobrem namespaces/privacidade,
  tipos nominais privados homônimos, transitive BorrowMut, captures/liveness,
  Rust FFI, manifesto/path/ciclos/boundaries, snapshots e stdout Cargo E2E.
