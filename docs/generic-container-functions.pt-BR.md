# Funções genéricas sobre containers

[English](generic-container-functions.md) · [Guia de uso](../README.pt-BR.md)

Uma função genérica cuja assinatura menciona um container genérico pode ser usada com argumentos de tipo explícitos. O compilador especializa a função para cada aplicação concreta antes da análise semântica:

```lift
interface Box<T> { value: T }

function unwrap<T>(box: Box<T>): T { return box.value; }
function unwrapTwice<T>(box: Box<Box<T>>): T {
    return unwrap::<Box<T>>(box).value;
}

const boxed: Box<int> = { value: 41 };
console.log(unwrap::<int>(boxed));       // 41
const nested: Box<Box<string>> = { value: { value: "hi" } };
console.log(unwrapTwice::<string>(nested)); // hi
```

## Como funciona

- O especializador encontra funções genéricas cujos parâmetros/retorno contêm aplicações genéricas (por exemplo `Box<T>`).
- Cada chamada explícita `f::<Args>(...)` cria uma cópia concreta da função com os parâmetros de tipo substituídos na assinatura, anotações locais, tipos de closure e chamadas explícitas aninhadas. A cópia é nomeada pela aplicação (`unwrap<int>`, `unwrapTwice<string>`), não tem parâmetros de tipo e passa pelo monomorfizador de containers.
- Chamadas aninhadas como `unwrap::<Box<T>>` dentro de `unwrapTwice<T>` são substituídas primeiro (`unwrap::<Box<string>>`) e especializadas recursivamente.
- Chamadas sem argumentos explícitos não são especializadas: o template original é removido, então `unwrap(boxed)` vira função desconhecida. Use `unwrap::<int>(boxed)`.

## Limites

- Argumentos explícitos são obrigatórios nesta categoria; não há inferência de `Box<int>` para `T`.
- Cópias especializadas são funções nomeadas comuns; o template original não pode ser usado como valor, callback ou sem argumentos.
- Funções genéricas sobre containers não são genéricas sobre o template além do mecanismo de especialização; ainda não há métodos/`impl` genéricos, containers genéricos em FFI ou API nativa de Map.
- Nomes gerados são internos (`unwrap<int>`); diagnósticos os exibem como nomes de função.

Testes: `crates/lift_sema/tests/generic_container_functions_contract.rs`; exemplo [`examples/generic-container-functions.lift`](../examples/generic-container-functions.lift).
