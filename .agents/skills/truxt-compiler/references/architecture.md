# Blueprint técnico do Truxt v0.1

Esta referência consolida o blueprint fornecido pelo usuário. Ela descreve o alvo do compilador, não prova que a implementação atual já satisfaça esses contratos. Em conflito com o código ou com uma instrução posterior do usuário, descreva a diferença e siga o escopo autorizado para a tarefa.

## Objetivo e workspace

Fluxo pretendido: fonte `.truxt` → AST → HIR tipado → código Rust → binário nativo.

## Compatibilidade TypeScript/Rust

Truxt busca familiaridade de sintaxe e semântica com TypeScript em um subconjunto
explicitamente suportado; não promete compatibilidade completa com TypeScript.
Cada recurso novo deve documentar sua semântica Truxt, limites em relação ao
TypeScript e lowering para Rust, preservando comportamento previsível e tipos
válidos. Ownership, borrowing, segurança e regras de tipos do Rust podem limitar
o subconjunto, mas não devem introduzir diferenças silenciosas. Valide o código
Rust gerado com Cargo/rustc nos exemplos e casos de contrato afetados antes de
afirmar compatibilidade.

```text
truxt/
├── Cargo.toml
├── crates/
│   ├── truxt_lexer/     # fonte -> tokens com spans
│   ├── truxt_ast/       # nós da AST, sem lógica
│   ├── truxt_parser/    # tokens -> AST não tipada
│   ├── truxt_sema/      # símbolos, tipos, análise de ownership -> HIR
│   ├── truxt_codegen/   # HIR -> Rust e manifesto Cargo
│   ├── truxt_project/   # sessão, grafo, overlays, diagnósticos e lint HIR
│   └── truxt_cli/       # build, run, check, init
└── stdlib/
    └── truxt_core/      # Console, helpers de array e operações de string
```

## Roadmap e limite de cada fase

| Fase | Escopo previsto | Critério de aceite descrito no blueprint |
| --- | --- | --- |
| 1 — Core Syntax & Primitivos (MVP) | Lexer com `logos`; parser recursivo descendente e Pratt para expressões; `const`/`let`, funções, `if`/`else`, `while`; `number`, `int`, `string`, `boolean`, `void`. | Fibonacci recursivo e iterativo gera `.rs`. |
| 2 — Estruturas e objetos | `interface`, object literals, acesso a campos, `impl` e arrays (`T[]` → `Vec<T>`). | Manipular produtos, filtrar e somar valores. |
| 3 — Semântica e auto-borrow | Escopos/tabela de símbolos, inferência bidirecional, análise de mutação e modos de ownership. | Passar objetos entre funções sem escrever `&`/`.clone()` no Truxt e produzir Rust aceito por `rustc`. |
| 4 — Optionais e erros | `T?` → `Option<T>`, `??`, unions/`enum`, `try`/`catch` ou `Result` transparente. | Tratar operações falíveis sem `.unwrap()` manual. |
| 5 — Biblioteca e interop | Métodos estilo JS/TS, `console.log`, template strings e FFI/import de crates Rust. | Consumir uma crate Rust real. |
| 6 — CLI e diagnósticos | `clap`, diagnósticos `ariadne`, cache `.truxt_cache/` e invocação de `cargo build --release`. | `truxt run app.truxt` produz e executa o binário nativo. |

O blueprint também sugere criar AST e lexer com tokens/nós para objetos, funções, arrays e operadores na etapa inicial. Isso pode ser preparação estrutural; não significa que parser, semântica e codegen dessas funcionalidades estejam entregues na Fase 1. Mantenha explícito qual camada e qual fase passam a suportá-las.

Estado do incremento posterior à v0.1: a CLI já usa Ariadne 0.6.0 com spans em
bytes e preserva arquivos gerados de conteúdo idêntico. Isso não é cache completo
por chave de conteúdo. Módulos relativos explícitos, SourceId e tabela global
já estão implementados no caminho legado flat. Manifesto/namespaces locais
adicionais seguem `forklift-decisions.md`. O contrato legado
está em `modules-decisions.md`. Os demais contratos estão em `codegen-cli-decisions.md` e nos guias
`docs/diagnostics-and-build.md` / `.pt-BR.md` da raiz.

O incremento atual exige Rust/Cargo 1.94.1+ e acrescenta métodos nominais/impl,
closures locais com captura snapshot e templates primitivos contextuais. Consulte
`methods-closures-templates-decisions.md` para os contratos, diferenças em relação
ao TypeScript e limites de ownership; callbacks/escape de closure seguem fora.

## Pipelines

O incremento de tooling extrai a carga de projeto e SourceMap da CLI para
`truxt_project`, preservando APIs/semântica anteriores. CLI, lint e LSP inicial
consomem a mesma sessão; veja `tooling-decisions.md` e os guias bilíngues
`docs/project-analysis-and-lsp.md` / `docs/project-analysis-and-lsp.pt-BR.md`.

### A. Fonte para AST

1. O loader lê o grafo `.truxt`, mantém textos/caminhos no SourceMap da CLI e fornece `Span { source: SourceId, start: usize, end: usize }`. Construtores/APIs legadas continuam com fonte zero.
2. `truxt_lexer` produz `Vec<(Token, Span)>`; cada nó AST deve preservar seu `Span`. O blueprint prevê interpolação de template string em partes de texto e expressão.
3. `truxt_parser` usa recursive descent para declarações e Pratt parsing para expressões binárias e pós-fixadas (`.`, `()`, `[]`), produzindo `UntypedAST`.

Feche um contrato de span compartilhado entre crates. A unidade de offset (bytes ou caracteres), intervalos, spans de EOF/erros e acesso ao lexema precisam ser consistentes. As assinaturas propostas não passam `src` ao parser; portanto, o conteúdo dos identificadores/literais precisa estar disponível nos tokens ou a API deve ser ajustada explicitamente. Não crie tipos de span incompatíveis em crates diferentes.

### B. AST para HIR tipado

A semântica usa checagem bidirecional:

- **Síntese (bottom-up):** por exemplo, literal `10` infere `int` (`i64`).
- **Verificação (top-down):** uma anotação esperada, como `User`, orienta a resolução de `{ name: "Ana" }` para uma inicialização da struct correspondente.

Mesmo na Fase 1, o critério de compilar Fibonacci requer semântica mínima para resolver nomes, chamadas (incluindo recursão), tipos de operadores e retornos. A Fase 3 aprofunda escopos, inferência e ownership; não adie toda checagem semântica até lá. O limite exato dessa semântica inicial deve acompanhar o recurso aceito na fase.

### C. Inferência de memória

O blueprint indica os modos pretendidos:

- Parâmetro somente lido: `BorrowImm`; Rust recebe `&T` e a chamada injeta `&arg`.
- Parâmetro com campo alterado: `BorrowMut`; Rust recebe `&mut T` e a chamada injeta `&mut arg`.
- Valor retornado ou armazenado em outro lugar: a função precisa de ownership (`OwnedMove`). Se o chamador ainda precisa do valor, a chamada pode clonar para passar ownership, desde que o tipo seja clonável.
- Atribuição local de tipo não-`Copy` com usos posteriores da origem: inserir clone automático, se necessário para evitar move.

Esses exemplos são intenção de alto nível, não um algoritmo completo. O blueprint não define análise de liveness/CFG, conflitos de alias, propagação de mutabilidade, fronteiras de escape nem todas as regras para parâmetros e retornos. Não implemente heurísticas locais como se provassem segurança geral. Delimite casos suportados, adicione diagnóstico para casos não resolvidos e registre as decisões que ampliem a semântica.

Não confunda o modo da assinatura de um parâmetro (`BorrowImm`, `BorrowMut` ou `OwnedMove`) com o modo de um argumento na chamada (`ArgPassMode`: `ByValue`, `ByRef`, `ByMutRef`, `ByClone`). A tabela informal do blueprint também usa `Cloned` ao descrever o parâmetro; esse rótulo é ambíguo. Como interpretação provisória, trate clone como decisão do ponto de chamada, representada por `ByClone`, e registre essa decisão até haver um contrato mais específico. O HIR deve representar decisões de ownership também em atribuições, retornos, campos e elementos de arrays, para que o codegen não precise redescobri-las. Só insira clone quando a semântica de valor/identidade estiver definida e o tipo for comprovadamente clonável.

### D. Lowering e Cargo

Transformações exemplificadas: concatenação com `+` para `format!`, `.map()` para `iter().map(...).collect::<Vec<_>>()`, e `console.log` para `println!`. O codegen grava `.truxt_cache/src/main.rs` e `.truxt_cache/Cargo.toml`; a CLI invoca Cargo, captura avisos/saída e copia ou executa o binário conforme o comando solicitado.

Essas traduções dependem de tipos e ownership: `.map()` não implica sempre `.iter()`, uma closure pode consumir ou emprestar elementos, e strings podem exigir `String` ou empréstimo conforme uso/mutação. Preserve o requisito de `derive(Clone, Debug, PartialEq)` nas structs e verifique que os tipos de todos os campos satisfazem os traits gerados. Como a assinatura proposta de `emit_rust` retorna `String`, a semântica deve rejeitar HIR inválido ou recursos ainda não suportados antes do codegen, em vez de esperar que `rustc` descubra erro de linguagem Truxt.

## Stack proposta

- Host: Rust, Edition 2021 ou superior.
- Lexer: `logos`.
- Diagnósticos: `ariadne`, com spans baseados em `Range<usize>`.
- CLI: `clap` com derive.

## Gramática resumida recebida

```ebnf
Program       ::= Decl*
Decl          ::= InterfaceDecl | FnDecl | ImportDecl | ModuleImportDecl | Stmt
InterfaceDecl ::= "interface" Ident "{" (Ident ":" Type ","?)* "}"
FnDecl        ::= "function" Ident "(" ParamList? ")" (":" Type)? Block
ImportDecl    ::= "import" "function" Ident "(" ParamList? ")" ":" Type "from" StrLit ";"
ModuleImportDecl ::= "import" "module" StrLit ";"
ParamList     ::= Param ("," Param)*
Param         ::= Ident ":" Type
Block         ::= "{" Stmt* "}"
Stmt          ::= VarDecl | AssignStmt | IfStmt | WhileStmt | ReturnStmt | ExprStmt
VarDecl       ::= ("const" | "let") Ident (":" Type)? "=" Expr
AssignStmt    ::= Expr "=" Expr
IfStmt        ::= "if" Expr Block ("else" (IfStmt | Block))?
WhileStmt     ::= "while" Expr Block
ReturnStmt    ::= "return" Expr?
Expr          ::= BinaryExpr | PostfixExpr | PrimaryExpr
PostfixExpr   ::= PrimaryExpr ("." Ident | "(" ArgList? ")" | "[" Expr "]")*
PrimaryExpr   ::= IntLit | FloatLit | StrLit | BoolLit | Ident | ObjectLit | ArrayLit | "(" Expr ")"
```

Esta gramática é um resumo, não uma especificação completa: não detalha operadores/precedências, regras de atribuição, tipos opcionais, unions, `impl` ou template strings. O contrato limitado de imports/FFI e `console.log` está em `phase5-decisions.md`; ele não define imports arbitrários, generics ou tipos struct. A existência de uma funcionalidade no roadmap sem produção gramatical correspondente é uma lacuna a resolver no escopo da fase, não licença para inventar sintaxe silenciosamente.

## Mapeamento de tipos fornecido

| Truxt | Rust pretendido |
| --- | --- |
| `int` | `i64` (`Copy`) |
| `number` | `f64` (`Copy`) |
| `boolean` | `bool` (`Copy`) |
| `string` | `String` em variáveis/structs; `&str` em parâmetros `BorrowImm` |
| `void` | `()` |
| `T[]` | `Vec<T>` |
| `T?` | `Option<T>` |
| tipo customizado | `T`, `&T` ou `&mut T`, conforme o modo de passagem |

## Contratos entre crates

```rust
truxt_lexer::tokenize(src: &str)
    -> Result<Vec<(Token, Span)>, LexError>
truxt_parser::parse(tokens: &[(Token, Span)])
    -> Result<ProgramAst, Vec<ParseError>>
truxt_sema::analyze(ast: ProgramAst)
    -> Result<HirProgram, Vec<SemaError>>
truxt_codegen::emit_rust(hir: &HirProgram)
    -> String
```

Estas são as assinaturas exatas fornecidas como contratos-alvo. Confira a implementação antes de depender delas; se houver divergência, registre-a e alinhe os consumidores dentro do escopo solicitado. Não altere uma assinatura silenciosamente.

## Invariantes obrigatórios do HIR

1. Toda expressão HIR carrega seu `ResolvedType` validado.
2. Todo object literal AST é resolvido para `HirExpr::StructLiteral { struct_name, fields }`.
3. Cada argumento de chamada HIR registra `ArgPassMode`: `ByValue`, `ByRef`, `ByMutRef` ou `ByClone`.
4. Toda struct gerada deriva `Clone`, `Debug` e `PartialEq`.

## Lacunas que não devem virar requisitos implícitos

- Semântica de `const` versus `let` e mutabilidade de bindings.
- Faixas exatas de precedência/associatividade e parsing de atribuição.
- Unidade/intervalo de offsets, spans de EOF/erros e contrato para lexemas no lexer/parser.
- Contrato do lexer para template strings com `logos` (especialmente estados e spans das interpolações).
- Tipos de retorno inferidos, coerções, promoção numérica e conversões entre `int` e `number`; semântica de divisão inteira, overflow e condições.
- Mutabilidade de `const` versus mutabilidade dos campos, e semântica de identidade/valor ao atribuir tipos não-`Copy` ou inserir `.clone()`.
- Regras completas de ownership, borrowing, clonagem, liveness e aliasing, inclusive chamadas transitivas, recursão, caminhos condicionais e argumentos que referem o mesmo objeto.
- Inferência nominal para object literals sem tipo esperado ou diagnóstico correspondente.
- Tratamento concreto de `try`/`catch` versus `Result` e construção/seleção de variantes de unions.
- Imports/FFI gerais continuam fora do contrato: o MVP da Fase 5 declara assinaturas explícitas para funções seguras e tipos limitados; adicionar uma crate ao Cargo não fornece por si só assinaturas nem mapeamento de tipos.
- Política de dependências, chave do cache para entradas/configurações e estrutura final do Cargo gerado.
- Nome final do comando CLI (`truxtc` aparece no exemplo inicial de comandos; o exemplo de execução usa `truxt`).

Se uma tarefa tocar uma dessas áreas, documente decisão, abrangência e limitações no código/documentação pertinente; não apresente o comportamento como definido pelo blueprint quando foi escolhido durante a implementação.
