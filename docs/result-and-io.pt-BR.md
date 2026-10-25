# Result e I/O UTF-8 na v0.1

[English](result-and-io.md) · [Guia de uso](../README.pt-BR.md)

`Result<T, E>` é um tipo especial embutido, convertido para `std::result::Result<T, E>` do Rust. É aceito em anotações de variáveis, campos de interfaces, arrays e parâmetros/retornos de funções Truxt. Tipos Result aninhados são aceitos em Truxt. Isso não adiciona tipos genéricos gerais.

`void` pode ser o tipo de sucesso (`Result<void, string>` → `Result<(), String>`). Continua inválido como erro, elemento de array, variável isolada ou parâmetro comum. Ciclos diretos de structs por Result são rejeitados como os ciclos por campo direto; uma indireção por array continua quebrando esse ciclo de tamanho.

## Tratar um resultado

```truxt
import function read_file(path: string): Result<string, string> from "truxt_core::io::read_file";

const contents: Result<string, string> = read_file("input.txt");
if result.is_ok(contents) {
    console.log(result.unwrap(contents));
} else {
    console.log(result.unwrap_err(contents));
}
```

`result.is_ok(value)` empresta o Result e retorna `boolean`. `result.unwrap(value)` retorna uma cópia própria do valor de sucesso; `result.unwrap_err(value)` retorna uma cópia própria do erro. O payload é clonado, preservando o Result original. Um unwrap de sucesso de `Result<void, E>` é uma expressão void utilizável como statement. Cada expressão de argumento é avaliada uma vez.

Unwrap na variante errada causa panic. O compilador não prova que o ramo verificou a variante; o chamador deve usar `is_ok` adequadamente. O namespace `result` não pode estar sombreado por variável, função ou interface. Ainda não há construtores Result em Truxt: os valores vêm de imports ou são passados/retornados por funções Truxt. Pattern matching, `?`, `try/catch`, unions, opcionais e generics gerais continuam pendentes.

## Contrato FFI

Um import pode retornar `Result<T, E>` quando `T` é primitivo suportado, array raso de primitivos ou `void`, e `E` é primitivo suportado ou array raso de primitivos. Retornos Result aninhados e parâmetros Result não são suportados no FFI. Strings nos payloads são `String` owned; arrays são `Vec<T>` owned. O wrapper com ponteiro de função seguro deixa Cargo/rustc verificar o retorno Rust completo.

## Funções da biblioteca

| Destino do import | Retorno Truxt | Comportamento |
| --- | --- | --- |
| `truxt_core::io::arguments` | `Result<string[], string>` | Exclui argv[0]; preserva ordem, argumentos vazios, flags e espaços. Unicode inválido vira erro. |
| `truxt_core::io::read_stdin` | `Result<string, string>` | Lê até EOF; preserva quebras de linha; rejeita UTF-8 inválido. |
| `truxt_core::io::read_file` | `Result<string, string>` | Lê um arquivo UTF-8 inteiro. |
| `truxt_core::io::write_file` | `Result<void, string>` | Cria ou trunca arquivo UTF-8; não cria diretórios pais. |

Caminhos são strings. Caminhos relativos usam o diretório de trabalho do chamador, preservado por `truxt run`. Mensagens de erro incluem operação, caminho quando aplicável e erro de sistema/UTF-8. Este incremento não oferece códigos de erro estruturados. A escrita não é atômica e uma falha pode deixar arquivo parcial. Os bindings retornam erros em vez de encerrar o processo; unwrap na variante errada e o binding atual de console ainda podem causar panic.

Use `truxt run app.truxt -- --flag "a b"` para encaminhar flags e argumentos com espaços. O compilador não consome stdin canalizado durante a compilação. Consulte `examples/io.truxt` para tratar erros de leitura/escrita. Testes unitários e de CLI cobrem sucesso, stdin vazio/multilinha, argumentos, arquivos Unicode, truncamento, arquivos/diretórios ausentes, UTF-8 inválido e uso incorreto explícito de unwrap.
