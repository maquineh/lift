# API de projeto, lint e diagnósticos no editor

[English](project-analysis-and-lsp.md) · [Guia de uso](../README.pt-BR.md)

`lift_project` é a fachada reutilizável do frontend para CLI, lint HIR e LSP
inicial. APIs existentes de lexer/parser/sema/codegen e a semântica HIR permanecem
iguais. Build/check/run consomem a fachada antes das etapas Rust/Cargo existentes.

O [incremento Forklift local](forklift.pt-BR.md) acrescenta `lift.toml`, análise
contextual de pacotes, namespaces e funções exportadas. APIs mantêm assinaturas;
enums AST ganham variantes para a nova sintaxe.

```rust
use lift_project::{ProjectSession, lint};

let mut session = ProjectSession::new();
session.set_overlay("main.lift", "console.log(7);".to_owned())?;
let analysis = session.analyze("main.lift");
if let Some(hir) = &analysis.hir {
    let warnings = lint::lint(hir);
    // emit_rust(hir), ou consuma o programa tipado em outro frontend.
}
// sources e diagnostics continuam disponíveis na falha.
# Ok::<(), std::io::Error>(())
```

A sessão possui overlays completos indexados pela grafia absoluta estável,
preservando `..` em vez de colapsá-lo através de symlinks. A API preserva a grafia;
a análise resolve aliases pelo filesystem quando possível. Caminhos ainda não
resolvidos mantêm a grafia e não são presumidos canonicamente equivalentes.
Aliases resolvíveis pelo filesystem são resolvidos por análise; salvar pela
primeira vez sob symlink/junction preserva o buffer. Se múltiplos overlays resolvem
ao mesmo arquivo canônico, vence a atualização mais recente. Remova o overlay
com a mesma grafia absoluta da abertura. Substituem o texto de disco na entrada e imports
transitivos, inclusive arquivos novos não salvos. `remove_overlay` retoma leituras
de disco. Cada `analyze` recarrega o grafo sem gravar arquivos, chamar Cargo ou
acessar registry. Caminhos relativos usam o diretório atual do processo; prefira
absolutos. Deduplicação canônica, ciclos, declarações globais e statements
executáveis exclusivos da entrada preservam os [contratos legados de módulos](modules.pt-BR.md).
Projetos manifestados usam [fronteiras de pacote](forklift.pt-BR.md): módulos
continuam flat dentro de cada pacote; dependências expõem funções exportadas.

`Analysis` possui caminho da entrada, `SourceMap`, HIR opcional e diagnósticos
estruturados. HIR existe somente se todo o grafo passar carga/lexer/parser/semântica.
Falhas retêm fontes lidas sem HIR parcial. Erros independentes de carga de imports
podem ser coletados. `Diagnostic` contém código, severidade (`Error`/`Warning`),
mensagem, span primário opcional e secundário opcional. Erros I/O/extensão da entrada
não têm span; falhas ao ler módulo apontam ao literal do importador. Códigos usam
`project.*`, `package.*`, `lexer.invalid-token`, `parser.syntax` e `sema.<SemaErrorKind>` por
mapeamento explícito estável, independente de Debug Rust;
mensagens são apresentação. Spans continuam bytes UTF-8 semiabertos. IDs pertencem
a uma análise e devem ser resolvidos por seu mapa, inclusive labels secundários.
Resultados anteriores continuam snapshots próprios após mudanças da sessão.
Não há cache incremental de análise.

HIR validado é garantia dos resultados de `ProjectSession::analyze`. Campos de
HIR/Analysis continuam públicos e podem ser reconstruídos; valores montados
manualmente não adquirem validação somente por usar esses tipos.

`SourceFile::position_utf16` converte bytes para linha/coluna UTF-16 base zero,
incluindo Unicode astral e quebras LF, CRLF ou CR isolado. Offsets fora da fonte são limitados ao texto;
offsets dentro de code points UTF-8 recuam até sua fronteira. Caminhos Rust são
nativos do sistema; LSP aceita URIs `file://` UTF-8, escapes percentuais e caminhos
drive/UNC do Windows.

## Lint

```sh
lift lint examples/modules/main.lift
lift lint --deny-warnings examples/modules/main.lift
```

Lint carrega e verifica tipos do grafo completo, depois percorre HIR validado.
Não emite Rust, cria `.lift_cache` ou verifica imports Rust com Cargo. Avisos saem
0 por padrão; `--deny-warnings` sai 1 se houver avisos. Erros fonte/sema continuam
erros e saem 1. `lift check` continua validando Rust gerado, caminhos e assinaturas.

| Regra | Comportamento consultivo |
| --- | --- |
| `unused-binding` | Variável local ou parâmetro sem referência HIR. Usa IDs, initializers de captura e corpos de closures; escritas contam como referências. Prefixos `_` e `this` implícito de métodos são isentos. Sem análise de alcançabilidade/dead stores. |
| `literal-condition` | Condição `if`/`while` é boolean literal, possivelmente agrupado. `while true` intencional também avisa. |
| `discarded-value` | Statement de expressão descarta literal ou local direto, possivelmente agrupado. Chamadas e expressões com possíveis efeitos não disparam. |

Regras não removem initializers, reescrevem fonte, inferem visibilidade ou alteram
decisões de ownership/codegen. Também visitam corpos de closures locais.

## LSP inicial e VS Code

`lift lsp` usa JSON-RPC em stdio com framing Content-Length e logs em stderr.
Anuncia full-sync e posições UTF-16. Suporta initialize/initialized/shutdown/exit,
didOpen/didChange/didSave/didClose e workspace/didChangeWatchedFiles. Outras
requisições retornam method-not-found. Publica erros frontend, locais relacionados
e avisos de lint, sem Cargo. Completion, hover, navegação, formatação, fixes e
debugging continuam futuros.

Frames completos com JSON inválido recebem parse error JSON-RPC (-32700);
envelopes inválidos recebem -32600. A sessão continua. Falhas irrecuperáveis de
framing/transporte encerram com mensagem em stderr e saída 1.

Defina `initializationOptions.entry` como caminho absoluto ou URI de arquivo para
um projeto. Sem isso cada documento seleciona seu projeto manifestado ancestral
ou entrada legada. Fonte não importada sob manifesto recebe `package.outside-graph`.
Entry explícita limita diagnósticos ao seu grafo; outros arquivos abertos ficam
fora do escopo selecionado. Projetos configurados reanalisam todo o grafo com
todos os overlays. Publicações incluem versão do documento aberto; versões
didChange atrasadas/não crescentes são ignoradas. Documentos preservam a identidade
da URI original ao salvar pela primeira vez sob symlink/junction; canonicalização
apenas resolve aliases da análise. Se múltiplas URIs abertas resolvem ao mesmo
arquivo, o último didOpen/didChange aceito entre elas determina buffer autoritativo,
URI do diagnóstico e versão. Os demais aliases recebem diagnósticos vazios.
Fechar a URI autoritativa restaura o buffer restante atualizado mais recentemente,
inclusive URIs com escapes diferentes para o mesmo caminho decodificado.
A análise é síncrona, sem disputa
de resultados em background com snapshots novos. Diagnósticos são limpos quando
corrigidos ou quando o arquivo sai do grafo. Fechar remove overlay e retoma disco
em projetos configurados. Eventos de arquivos monitorados recarregam dependências
de disco. A reanálise atual é completa, sem debounce.

A [extensão local VS Code](../editors/vscode/README.pt-BR.md) contém cliente stdio,
grammar TextMate com interpolação de templates, regras de comentários/brackets e
tasks check/build/lint/run. Configure `lift.serverPath` e, para módulos,
`lift.entry`. A primeira pasta do workspace resolve entradas relativas; múltiplos
projetos não têm roteamento próprio. Tasks consomem arquivos salvos; LSP consome
overlays. Mudanças de configuração reiniciam o cliente. A extensão não foi publicada.

## Próximos incrementos

Forklift local com dependências path, manifestos e namespaces está implementado;
veja [seu escopo exato](forklift.pt-BR.md). A PoC local de notas adiciona
adaptadores SQLite/HTTP, sem bindings de handles ou callbacks externos em Lift.
Handlers Lift, uma API de banco geral e DAP continuam próximos. Compatibilidade
PostgreSQL 8.4/9.1 deve ser validada antes de escolher driver; não se presume que
drivers modernos suportem esses servidores.
