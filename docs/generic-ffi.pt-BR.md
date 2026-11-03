# Layouts externos genéricos (FFI)

[English](generic-ffi.md) · [Guia de uso](../README.pt-BR.md)

Interfaces importadas podem ser genéricas. Cada aplicação concreta vira um layout externo com caminho Rust monomorfizado:

```lift
import interface Wrapper<T> { value: T } from "lift_core::collections::Wrapper";
import function wrap_i64(value: int): Wrapper<int> from "lift_core::collections::wrap_i64";
import function unwrap_i64(wrapper: Wrapper<int>): int from "lift_core::collections::unwrap_i64";
import function wrap_string(value: string): Wrapper<string> from "lift_core::collections::wrap_string";
import function unwrap_string(wrapper: Wrapper<string>): string from "lift_core::collections::unwrap_string";

const wrapped_number: Wrapper<int> = wrap_i64(41);
console.log(unwrap_i64(wrapped_number)); // 41

const wrapped_text: Wrapper<string> = wrap_string("hi");
console.log(unwrap_string(wrapped_text)); // hi
```

## Como funciona

- `import interface Nome<T> { ... } from "crate::path"` declara um template externo genérico. Aplicações `Nome<Arg>` viram declarações sintéticas de layout externo (`Wrapper<int>`, `Wrapper<string>`) com campos substituídos e caminho Rust `crate::path<i64>`/`crate::path<String>`.
- Argumentos de tipo atualmente mapeiam apenas primitivos/string: `int -> i64`, `number -> f64`, `string -> String`, `boolean -> bool`. Arrays, containers, options e generics aninhados são rejeitados como argumentos externos.
- Os layouts resultantes seguem as regras normais de FFI: structs externas em parâmetros/retornos, campos verificados pelo codegen e funções resolvidas pela dependência Rust.
- O template genérico é removido após a instanciação; `Wrapper` sem argumentos é desconhecido. Cada aplicação é nominal e cacheada.
- `lift_core::collections::Wrapper<T>` com `wrap_i64`/`unwrap_i64`/`wrap_string`/`unwrap_string` fornece um exemplo executável.

## Limites

- Apenas argumentos primitivos/string/boolean; sem `Wrapper<int[]>`/`Box<int>` como argumento externo genérico.
- Sem construção de layouts externos genéricos no Lift (eles cruzam a FFI), sem funções FFI genéricas, sem impls genéricos em layouts externos e sem API nativa de Map ainda.
- Layouts externos genéricos seguem as restrições usuais de FFI (Option/Result, ownership, callbacks).

Testes: `crates/lift_sema/tests/generic_ffi_contract.rs`; exemplo [`examples/generic-ffi.lift`](../examples/generic-ffi.lift).
