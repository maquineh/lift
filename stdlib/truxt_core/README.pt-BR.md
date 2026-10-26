# `truxt_core`

Rust/Cargo **1.94.1+** (MSRV: `1.94.1`).

`truxt_core` é a biblioteca base Rust-native do Truxt. Ela não tem dependências externas e oferece saída de console e helpers de arrays e strings.

## Uso em Truxt

Imports Truxt declaram um nome local e uma assinatura completa. O módulo `interop` contém adaptadores com assinaturas Rust compatíveis com o contrato FFI seguro atual:

```truxt
import function tamanho(values: int[]): int from "truxt_core::interop::array_len";
import function trim(value: string): string from "truxt_core::interop::string_trim";
import function contem(values: int[], value: int): boolean from "truxt_core::interop::array_contains_int";
import function tem_texto(value: string, parte: string): boolean from "truxt_core::string::contains";

const values: int[] = [2, 4, 6];
console.log(tamanho(values));
console.log(contem(values, 4));
console.log(trim("  Truxt  "));
console.log(tem_texto("Olá, Truxt", "Truxt"));
```

Adaptadores disponíveis:

| Caminho Rust | Assinatura Truxt | Comportamento |
| --- | --- | --- |
| `interop::array_len` | `T[] -> int` | Retorna a quantidade de elementos como `i64`. |
| `interop::string_len` | `string -> int` | Conta valores escalares Unicode (`char`), não bytes nem grafemas. |
| `interop::string_trim` | `string -> string` | Remove whitespace Unicode das extremidades e retorna uma string owned. |
| `interop::array_contains_int` | `(int[], int) -> boolean` | Verifica pertinência exata de `i64`. |
| `interop::array_contains_number` | `(number[], number) -> boolean` | Verifica igualdade exata de `f64`. |
| `interop::array_contains_boolean` | `(boolean[], boolean) -> boolean` | Verifica pertinência de booleanos. |
| `interop::array_contains_string` | `(string[], string) -> boolean` | Verifica pertinência exata de strings. |

`array::is_empty`, `string::is_empty`, `string::contains` e `string::concat` também são compatíveis com FFI. A CLI embute essas fontes e as extrai no cache do projeto gerado. Seu executável pode ser movido sem manter o checkout. Rust/Cargo e um linker do host continuam necessários.

## Argumentos, stdin e arquivos

```truxt
import function arguments(): Result<string[], string> from "truxt_core::io::arguments";
import function read_stdin(): Result<string, string> from "truxt_core::io::read_stdin";
import function read_file(path: string): Result<string, string> from "truxt_core::io::read_file";
import function write_file(path: string, contents: string): Result<void, string> from "truxt_core::io::write_file";

const text = read_file("input.txt");
if result.is_ok(text) {
    console.log(result.unwrap(text));
} else {
    console.log(result.unwrap_err(text));
}
```

Argumentos excluem argv[0] e exigem Unicode válido. Stdin é lido como UTF-8 até EOF, preservando quebras de linha. Arquivos usam UTF-8; escrita cria ou trunca sem criar diretórios pais. Caminhos relativos usam o diretório de trabalho do chamador. Erros são strings com operação, caminho quando relevante e erro original. A escrita não é atômica.

`result.is_ok` empresta o resultado; `result.unwrap` e `result.unwrap_err` clonam o payload. Unwrap na variante errada causa panic. Consulte [Result e I/O](../../docs/result-and-io.pt-BR.md) para limites e exemplos. Erros de I/O são tratáveis por Result; `console.log` ainda causa panic se stdout falhar.

## API Rust

A API Rust é mais ampla que a superfície FFI acessível por Truxt:

- `Console::log` e `Console::error` escrevem um valor `Display` seguido de quebra de linha e retornam `std::io::Result<()>`.
- `array` oferece `len`, `is_empty`, `contains`, `map`, `filter` e `find` sobre slices emprestados.
- `string` oferece `len`, `is_empty`, `contains`, `concat` e `trim`.
- `io` oferece `arguments`, `read_stdin`, `read_file` e `write_file`, todos retornando `Result<_, String>`.
- `map`, `filter` e `find` recebem callbacks `FnMut`. Closures locais com snapshot ainda não possuem tipo de função higher-order ou mapeamento FFI, então esses helpers continuam exclusivos do Rust.

`string::len` conta valores escalares Unicode; por exemplo, `"é🙂"` tem comprimento 2. Em Rust, `string::trim` retorna um `&str` emprestado. O adaptador FFI `interop::string_trim` retorna uma `String` owned para corresponder ao contrato atual de retorno de string em Truxt.

Execute os testes desta crate a partir da raiz do workspace com `cargo test -p truxt_core`.
