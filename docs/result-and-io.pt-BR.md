# Result e I/O UTF-8 na v0.1

[English](result-and-io.md) · [Guia de uso](../README.pt-BR.md)

`Result<T, E>` é um tipo especial embutido, convertido para `std::result::Result<T, E>` do Rust. É aceito em anotações de variáveis, campos de interfaces, arrays e parâmetros/retornos de funções Lift. Tipos Result aninhados são aceitos em Lift. Isso não adiciona tipos genéricos gerais.

`void` pode ser o tipo de sucesso (`Result<void, string>` → `Result<(), String>`). Continua inválido como erro, elemento de array, variável isolada ou parâmetro comum. Ciclos diretos de structs por Result são rejeitados como os ciclos por campo direto; uma indireção por array continua quebrando esse ciclo de tamanho.

## Tratar um resultado

```lift
import function read_file(path: string): Result<string, string> from "lift_core::io::read_file";

const contents: Result<string, string> = read_file("input.txt");
if result.is_ok(contents) {
    console.log(result.unwrap(contents));
} else {
    console.log(result.unwrap_err(contents));
}
```

`result.is_ok(value)` empresta o Result e retorna `boolean`. `result.unwrap(value)` retorna uma cópia própria do valor de sucesso; `result.unwrap_err(value)` retorna uma cópia própria do erro. O payload é clonado, preservando o Result original. Um unwrap de sucesso de `Result<void, E>` é uma expressão void utilizável como statement. Cada expressão de argumento é avaliada uma vez.

Unwrap na variante errada causa panic. O compilador não prova que o ramo verificou a variante; use `is_ok` adequadamente, ou os construtos `?`, `throw` e `try`/`catch`. O namespace `result` não pode estar sombreado. Valores Result agora podem ser construídos com `result.ok(value)`/`result.err(error)`; o operador de propagação `?`, `throw` e um `try`/`catch` tipado estão implementados com correspondência exata de tipo de erro e sem conversão `From` implícita. Matching de unions/enums e generics gerais continuam pendentes; `Option`/`Result` têm `match` exaustivo (veja [match de Option e Result](match.pt-BR.md)). Tipos opcionais e `??` seguem o [contrato separado de Option](option.pt-BR.md); o caminho de erro está em [erros e propagação](errors.pt-BR.md).

## Contrato FFI

Um import pode retornar `Result<T, E>` quando `T` é primitivo suportado, array raso, struct externa ou `void`, e `E` é primitivo suportado, array raso ou struct externa; também pode receber `Result<T, E>` como parâmetro emprestado (`&Result<...>`). Payloads usam o mapeamento owned e podem conter um `Option` direto (`Result<int?, string>` → `Result<Option<i64>, String>`). Result aninhado, `Option<Result<T, E>>`, Option dentro de array e parâmetro `Result<void, E>` não são suportados, e sucesso `void` só vale em retorno. O wrapper com ponteiro de função seguro deixa Cargo/rustc verificar a assinatura Rust completa. Consulte [structs externas e Option em imports Rust](ffi-structs-and-options.pt-BR.md) para a declaração de layout, parâmetros/retornos Option diretos e a matriz exata de aninhamento.

## Funções da biblioteca

| Destino do import | Retorno Lift | Comportamento |
| --- | --- | --- |
| `lift_core::io::arguments` | `Result<string[], string>` | Exclui argv[0]; preserva ordem, argumentos vazios, flags e espaços. Unicode inválido vira erro. |
| `lift_core::io::read_stdin` | `Result<string, string>` | Lê até EOF; preserva quebras de linha; rejeita UTF-8 inválido. |
| `lift_core::io::read_file` | `Result<string, string>` | Lê um arquivo UTF-8 inteiro. |
| `lift_core::io::write_file` | `Result<void, string>` | Cria ou trunca arquivo UTF-8; não cria diretórios pais. |

Caminhos são strings. Caminhos relativos usam o diretório de trabalho do chamador, preservado por `lift run`. Mensagens de erro incluem operação, caminho quando aplicável e erro de sistema/UTF-8. Este incremento não oferece códigos de erro estruturados. A escrita não é atômica e uma falha pode deixar arquivo parcial. Os bindings retornam erros em vez de encerrar o processo; unwrap na variante errada e o binding atual de console ainda podem causar panic.

Use `lift run app.lift -- --flag "a b"` para encaminhar flags e argumentos com espaços. O compilador não consome stdin canalizado durante a compilação. Consulte `examples/io.lift` para tratar erros de leitura/escrita. Testes unitários e de CLI cobrem sucesso, stdin vazio/multilinha, argumentos, arquivos Unicode, truncamento, arquivos/diretórios ausentes, UTF-8 inválido e uso incorreto explícito de unwrap.
