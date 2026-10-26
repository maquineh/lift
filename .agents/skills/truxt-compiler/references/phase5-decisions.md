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
snapshot e templates está em `methods-closures-templates-decisions.md`. Generics
gerais, callbacks FFI arbitrários, imports dinâmicos, structs FFI arbitrárias e unsafe continuam fora.

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

Arrays de string usam `&[String]`/`Vec<String>`; arrays aninhados, structs e
Option/T? não integra o FFI em nenhuma posição, inclusive retornos e dentro de Array/Result. Os argumentos HIR usam `ByValue` para
scalars e `ByRef` para strings/arrays. Retornos são owned. O fluxo existente de
ownership/clones continua aplicável aos valores retornados.

O incremento Result admite retorno `Result<T, E>` com T primitivo/array raso/void
e E primitivo/array raso. Parâmetros Result e retornos Result aninhados continuam
fora do FFI. Consulte `phase4-decisions.md` e os guias Result/I/O na pasta docs.

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
