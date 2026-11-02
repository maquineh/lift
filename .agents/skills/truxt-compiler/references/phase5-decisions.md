# Decisões do MVP da Fase 5: imports/FFI e console.log

Incremento SQL geral: `DbValue` é valor escalar clonável; `DbPool`,
`DbTransaction` e `DbRows` são variantes Resource afins. O catálogo fechado de
imports `lift_db::api` registra os modos de passagem; execute/query aceitam
`DbValue[]`. Pool configura SQLite/PostgreSQL, min_idle, máximo e timeout de
aquisição. `http.serve` pode consumir pool e handlers `(Request,DbPool)` apenas
o emprestam. O guia bilíngue `stdlib/lift_db/README.*` registra política TLS,
rollback/descarte e limites reais (CRUD/DDL, escalares, sem tradução SQL).

O incremento original implementou imports explícitos de funções Rust e o binding
de `console.log`. O incremento posterior de métodos nominais, closures locais
snapshot e templates está em `methods-closures-templates-decisions.md`; o
incremento higher-order/callbacks também está lá. Generics gerais, imports
dinâmicos, structs FFI sem layout declarado e unsafe continuam fora. Structs
externas e `T?` direto estão na seção final deste documento.

O incremento de recursos/HTTP usa `ResolvedType::Resource(ResourceKind)` e
`HirImport.parameter_modes` para um protocolo SQLite local fechado. Conexões e
transações têm ownership, empréstimos e descarte; não implementam Clone.
`http.route` valida DTOs e handlers nomeados em Lift; o codegen gera bridges sem
captura para `lift_web::http`. Semântica, limites e comandos de aceite estão em
`docs/resources-and-http.md` e `.pt-BR.md`. O contrato antigo de FFI primitiva
abaixo permanece válido para todos os imports externos ao protocolo fechado.

## Sintaxe e resolução

```lift
import function concat(a: string, b: string): string from "lift_core::string::concat";
import function f(value: int): int from "some-package@1.2.3::module::function";
console.log(concat("Olá, ", "Lift"));
```

- Imports são declarações de topo com nome local, parâmetros e retorno sempre
  explícitos; o semicolon é obrigatório. São pré-declarados como funções comuns,
  portanto chamadas podem aparecer antes do import. Não são valores de função.
- A AST conserva assinatura, destino e spans. A semântica resolve um `HirImport`
  com tipos, caminho Rust validado e `RustDependency`, compartilhando a tabela
  de funções e os diagnósticos de argumentos com funções Lift.
- `lift_core`, `lift_db` e `lift_web` sem versão identificam dependências locais
  embutidas; `lift_db`/`lift_web` são adaptadores separados da core. Outros
  pacotes exigem uma versão exata `MAJOR.MINOR.PATCH`, com componentes
  decimais u64 sem zeros à esquerda. Prereleases, ranges, URLs, git e paths
  externos não são aceitos. Cargo recebe `=MAJOR.MINOR.PATCH`.
- Pacotes usam um subconjunto conservador dos nomes Cargo: ASCII, letra inicial,
  no máximo 64 bytes, letras/dígitos/hífen/underscore. O nome Rust substitui `-`
  por `_`. Palavras reservadas Rust e nomes especiais/de artefatos Cargo,
  `std`, `core`, `alloc` e a versão registry de `lift_core` são rejeitados.
- Segmentos de caminhos devem ser identificadores Rust ASCII, sem palavras
  reservadas, `_`, raw identifiers, segmentos vazios ou escapes. Não há texto
  Rust/TOML arbitrário. Imports que mapeiam a mesma crate Rust para pacotes ou
  versões diferentes são rejeitados; dependências iguais são deduplicadas.

## Tipos e confirmação por rustc

| Lift | Parâmetro Rust | Retorno Rust |
| --- | --- | --- |
| int | i64 | i64 |
| number | f64 | f64 |
| boolean | bool | bool |
| string | &str | String |
| T[] (T primitivo acima) | &[T Rust owned] | Vec<T Rust owned> |
| void | proibido | () |

Arrays de string usam `&[String]`/`Vec<String>`; arrays aninhados e structs sem
declaração de layout continuam fora. A restrição original de Option/T? em
qualquer posição foi superada pelo incremento de structs externas/Option: hoje
`T?` direto é aceito em parâmetro e retorno. Os argumentos HIR usam `ByValue` para
scalars e `ByRef` para strings/arrays/structs/opcionais. Retornos são owned. O fluxo existente de
ownership/clones continua aplicável aos valores retornados.

O incremento Result admite retorno `Result<T, E>` com T primitivo/array raso/void
/struct externa e E primitivo/array raso/struct externa. Option dentro de Result
e parâmetros Result foram acrescentados no incremento v3 (seção final); retornos
Result aninhados continuam fora. Consulte `phase4-decisions.md` e os guias
Result/I/O na pasta docs.

Cada import gera um wrapper Rust privado com assinatura explícita. Dentro dele,
o destino é atribuído a um ponteiro `fn(...) -> ...` safe antes da chamada.
`rustc`, executado por check/build/run, confirma existência, visibilidade pública,
compatibilidade da assinatura e segurança da função externa. A análise Lift
confirma a assinatura declarada; não introspecta o código da crate Rust. Uma
função unsafe não coage ao ponteiro safe. A seleção/inferência de uma função
Rust genérica compatível pode ocorrer em Rust; não há sintaxe de generics Lift.

## console.log

- Apenas a chamada `console.log(expr)` com um argumento int, number, boolean ou
  string é builtin. O objeto `console` deve estar livre de bindings, funções e
  tipos declarados com esse nome. Um objeto local `console` continua sujeito
  às regras comuns, inclusive métodos nominais do incremento posterior.
- O HIR usa `HirExprKind::Call` com nome reservado `console.log`, tipo `void`
  e argumento `ByRef`. Assim walkers de chamadas/ownership existentes continuam
  a percorrer a expressão. O binding console não depende de métodos gerais;
  métodos nominais seguem methods-closures-templates-decisions.md.
- O codegen emite `::lift_core::Console::log(&arg).expect(...)`, com reborrow
  quando necessário. A expressão é avaliada uma vez e seu valor não é movido.
  O binding `console.log` mantém a política fail-fast em falha de stdout e não
  retorna `Result`, embora o tipo especial Result já esteja disponível em Lift.

## Manifest e CLI

- `emit_rust(&HirProgram) -> String` e `emit_cargo_manifest(&str) -> String`
  mantêm seus contratos. A nova `emit_cargo_manifest_for_program` recebe HIR e
  um caminho Cargo UTF-8 da crate core, adicionando dependências ordenadas.
  Caminhos relativos são resolvidos a partir do diretório do manifesto gerado;
  a CLI atualmente fornece `lift_core`, relativo ao `Cargo.toml` do programa.
- A CLI incorpora as fontes da stdlib com `include_str!` e as materializa no
  cache do programa, com caminho Cargo relativo `lift_core`. O core extraído
  não herda versão/edition do workspace original. O executável pode ser movido
  sem o checkout; Rust/Cargo e linker continuam pré-requisitos externos.
- `lift_core` só entra no manifesto quando existe import local ou uso builtin,
  inclusive em funções/ramos. Imports declarados e ainda não chamados também
  incluem dependência e wrapper para validar o contrato com rustc.
- Exemplos e E2E locais cobrem interop, stdlib, Result e I/O. O teste registry
  optativo faz check/build/run com `num-integer` 0.1.46 e confere stdout.
  A evidência cobre essa crate/caminho, sem alegar compatibilidade geral.

## Structs externas e Option direto v2

Guia público: `docs/ffi-structs-and-options.md` e `.pt-BR.md`.

- Sintaxe: `import interface Nome { campo: T, opcional?: T } from "destino";`.
  O destino usa `resolve_source` (lift_core/lift_db/lift_web locais ou
  `pacote@MAJOR.MINOR.PATCH::modulo::Tipo`), com as mesmas regras de caminho,
  versão, colisão de nome e conflito de crate já aplicadas a funções.
- HIR: `HirDeclaration::ExternStruct { name, fields, dependency, rust_path }`.
  Não gera `struct`; o codegen mapeia `ResolvedType::Struct(nome)` para o caminho
  Rust e trata o layout como opaco na análise de ciclo direto de tamanho.
  Nomes de campo precisam ser identificadores Rust (sem palavra reservada/raw) e
  são usados literalmente em literais, acesso e asserções.
- Campos: mapeamento `int→i64`, `number→f64`, `boolean→bool`, `string→String`,
  `T[]→Vec<T>`, struct externa→caminho Rust e `T?`/`?:`→`Option<T>`. Result,
  struct local Lift, recursos, array aninhado e opcional aninhado são rejeitados
  no layout externo. Campos `?:` continuam omitíveis e viram `None` no literal.
- Verificação: cada ExternStruct gera uma função de layout que clona cada campo
  para o tipo mapeado e uma asserção `Clone + Debug + PartialEq` sobre o caminho
  Rust; `rustc` confirma existência, visibilidade pública, tipo exato e traits.
  Layouts `#[non_exhaustive]`, campos privados/renomeados, campos extras e tipos
  divergentes falham no Rust gerado, sem remapeamento para a fonte Lift.
- FFI: parâmetros de struct externa viram `&P` e retornos `P`; arrays de struct
  externa usam `&[P]`/`Vec<P>`. `T?` direto em parâmetro vira
  `Option<&str>`, `Option<&[T]>`, `Option<&P>` ou `Option<escalar>`, e em retorno
  o option owned. O wrapper mantém a convenção Lift (`&Option<String>`,
  `&Option<Vec<T>>`, `Option<i64>` copiado por deref) e converte com `as_deref`,
  `as_ref` ou `*`. Result de retorno aceita payload de struct externa.
- Result v3: `Result<T, E>` também é aceito como parâmetro emprestado
  (`&Result<...>`) além de retorno owned. O payload usa o mapeamento owned e
  aceita um `Option` direto (`Result<int?, string>` → `Result<Option<i64>, String>`,
  `Result<string, string?>` → `Result<String, Option<String>>`). Um `Option` no
  payload de Result não muda a convenção de empréstimo do parâmetro: o Result
  inteiro é emprestado.
- Continuam rejeitados: Option dentro de Array, payload de Result aninhado
  (`Result<Result<...>, E>`), `Option<Result<T, E>>` (`Result<T, E>?`), Result
  dentro de array, `Result<void, E>` como parâmetro (`void` só em retorno),
  Option aninhado, array aninhado, struct local Lift em qualquer posição FFI,
  generics, enums, callbacks e unsafe.
- Testes: `crates/lift_sema/tests/ffi_structs_options_contract.rs`,
  `option_contract.rs`, `codegen_contract.rs` e o E2E de `examples/interop.lift`
  (struct emprestada/retornada, literal, array de structs, Option de escalar,
  string, array e struct, Result de struct, parâmetro Result emprestado com
  payload opcional e struct local com campos externos).
## Callbacks FFI v3

Contrato público: `docs/higher-order*.md`, `docs/ffi-structs-and-options*.md`.
Teste: `crates/lift_sema/tests/higher_order_contract.rs`, codegen e E2E de
`examples/higher-order.lift`.

- Tipo de função em import: parâmetro `(T) => R` vira `&dyn Fn(T Rust owned...)
  -> R Rust owned`; retorno `(T) => R` vira `Box<dyn Fn(...) -> ...>`, convertido
  para `Rc` pelo wrapper (`Rc::from`). O wrapper Lift recebe `Rc<dyn Fn>` por
  valor e empresta `&*arg` ao ponteiro de função segura.
- Payloads de callback usam o mapeamento owned (i64/f64/bool/String/Vec<T>/struct
  externa) e retorno `void` como `()`. `Option`, `Result`, outro tipo de função,
  recursos opacos e parâmetro `void` dentro da assinatura do callback são
  rejeitados antes do codegen.
- É contrato de trait object Rust (`&dyn Fn`/`Box<dyn Fn>`), não `extern "C"` nem
  ponteiro C; `FnMut`/`FnOnce`/`impl Fn` genérico exigem wrapper Rust.
- `apply_twice`, `apply_text` e `add_constant` em `lift_core::interop` exercitam
  callback emprestado, callback com String owned e callback retornado como Box.