# Decisões do MVP da Fase 5: imports/FFI e console.log

Este incremento implementa apenas imports explícitos de funções Rust e o binding
de `console.log`. Métodos gerais, closures, generics na sintaxe Truxt, template
strings, imports dinâmicos, structs FFI, unsafe e outros recursos do roadmap
continuam fora do escopo.

## Sintaxe e resolução

```truxt
import function concat(a: string, b: string): string from "truxt_core::string::concat";
import function f(value: int): int from "some-package@1.2.3::module::function";
console.log(concat("Olá, ", "Truxt"));
```

- Imports são declarações de topo com nome local, parâmetros e retorno sempre
  explícitos; o semicolon é obrigatório. São pré-declarados como funções comuns,
  portanto chamadas podem aparecer antes do import. Não são valores de função.
- A AST conserva assinatura, destino e spans. A semântica resolve um `HirImport`
  com tipos, caminho Rust validado e `RustDependency`, compartilhando a tabela
  de funções e os diagnósticos de argumentos com funções Truxt.
- `truxt_core` sem versão referencia exclusivamente a crate local do workspace.
  Outros pacotes exigem uma versão exata `MAJOR.MINOR.PATCH`, com componentes
  decimais u64 sem zeros à esquerda. Prereleases, ranges, URLs, git e paths
  externos não são aceitos. Cargo recebe `=MAJOR.MINOR.PATCH`.
- Pacotes usam um subconjunto conservador dos nomes Cargo: ASCII, letra inicial,
  no máximo 64 bytes, letras/dígitos/hífen/underscore. O nome Rust substitui `-`
  por `_`. Palavras reservadas Rust e nomes especiais/de artefatos Cargo,
  `std`, `core`, `alloc` e a versão registry de `truxt_core` são rejeitados.
- Segmentos de caminhos devem ser identificadores Rust ASCII, sem palavras
  reservadas, `_`, raw identifiers, segmentos vazios ou escapes. Não há texto
  Rust/TOML arbitrário. Imports que mapeiam a mesma crate Rust para pacotes ou
  versões diferentes são rejeitados; dependências iguais são deduplicadas.

## Tipos e confirmação por rustc

| Truxt | Parâmetro Rust | Retorno Rust |
| --- | --- | --- |
| int | i64 | i64 |
| number | f64 | f64 |
| boolean | bool | bool |
| string | &str | String |
| T[] (T primitivo acima) | &[T Rust owned] | Vec<T Rust owned> |
| void | proibido | () |

Arrays de string usam `&[String]`/`Vec<String>`; arrays aninhados, structs e
optionals não integram o contrato FFI. Os argumentos HIR usam `ByValue` para
scalars e `ByRef` para strings/arrays. Retornos são owned. O fluxo existente de
ownership/clones continua aplicável aos valores retornados.

O incremento Result admite retorno `Result<T, E>` com T primitivo/array raso/void
e E primitivo/array raso. Parâmetros Result e retornos Result aninhados continuam
fora do FFI. Consulte `phase4-decisions.md` e os guias Result/I/O na pasta docs.

Cada import gera um wrapper Rust privado com assinatura explícita. Dentro dele,
o destino é atribuído a um ponteiro `fn(...) -> ...` safe antes da chamada.
`rustc`, executado por check/build/run, confirma existência, visibilidade pública,
compatibilidade da assinatura e segurança da função externa. A análise Truxt
confirma a assinatura declarada; não introspecta o código da crate Rust. Uma
função unsafe não coage ao ponteiro safe. A seleção/inferência de uma função
Rust genérica compatível pode ocorrer em Rust; não há sintaxe de generics Truxt.

## console.log

- Apenas a chamada `console.log(expr)` com um argumento int, number, boolean ou
  string é builtin. O objeto `console` deve estar livre de bindings, funções e
  tipos declarados com esse nome. Um objeto local `console` continua sujeito
  às regras comuns, que não implementam chamadas de métodos.
- O HIR usa `HirExprKind::Call` com nome reservado `console.log`, tipo `void`
  e argumento `ByRef`. Assim walkers de chamadas/ownership existentes continuam
  a percorrer a expressão; métodos gerais não são introduzidos.
- O codegen emite `::truxt_core::Console::log(&arg).expect(...)`, com reborrow
  quando necessário. A expressão é avaliada uma vez e seu valor não é movido.
  O binding `console.log` mantém a política fail-fast em falha de stdout e não
  retorna `Result`, embora o tipo especial Result já esteja disponível em Truxt.

## Manifest e CLI

- `emit_rust(&HirProgram) -> String` e `emit_cargo_manifest(&str) -> String`
  mantêm seus contratos. A nova `emit_cargo_manifest_for_program` recebe HIR e
  um caminho Cargo UTF-8 da crate core, adicionando dependências ordenadas.
  Caminhos relativos são resolvidos a partir do diretório do manifesto gerado;
  a CLI atualmente fornece `truxt_core`, relativo ao `Cargo.toml` do programa.
- A CLI incorpora as fontes da stdlib com `include_str!` e as materializa no
  cache do programa, com caminho Cargo relativo `truxt_core`. O core extraído
  não herda versão/edition do workspace original. O executável pode ser movido
  sem o checkout; Rust/Cargo e linker continuam pré-requisitos externos.
- `truxt_core` só entra no manifesto quando existe import local ou uso builtin,
  inclusive em funções/ramos. Imports declarados e ainda não chamados também
  incluem dependência e wrapper para validar o contrato com rustc.
- Exemplos e E2E locais cobrem interop, stdlib, Result e I/O. O teste registry
  optativo faz check/build/run com `num-integer` 0.1.46 e confere stdout.
  A evidência cobre essa crate/caminho, sem alegar compatibilidade geral.
