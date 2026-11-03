# Argumentos de tipo explícitos

[English](explicit-type-arguments.md) · [Guia de uso](../README.pt-BR.md)

Funções genéricas normalmente inferem parâmetros de tipo pelos argumentos. Quando a inferência é ambígua ou a documentação se beneficia de explicitude, use o sufixo `::<...>` no callee:

```lift
function identity<T>(value: T): T { return value; }
function first<T>(values: T[]): T { return values[0]; }

console.log(identity::<int>(41));
console.log(identity::<string>("hi"));
console.log(first::<int>([1, 2]));

interface Box<T> { value: T }
const boxes: Box<int>[] = [{ value: 7 }];
const boxed = first::<Box<int>>(boxes);
```

## Regras

- O sufixo pertence à função chamada: `f::<T>(args)`. Só é válido imediatamente antes de uma chamada; usar `f::<T>` como valor é rejeitado.
- A quantidade de argumentos de tipo precisa coincidir com os parâmetros declarados. Cada argumento precisa ser um tipo Lift válido, incluindo containers (`Box<int>`) e enums genéricos.
- Argumentos explícitos semeiam a inferência; os argumentos reais continuam checados contra os tipos substituídos. Conflito (por exemplo `f::<int>("x")`) é type mismatch.
- Usar `::<...>` em função nomeada não-genérica é rejeitado.
- A chamada Rust gerada continua comum e depende da inferência do Rust; os argumentos explícitos são uma checagem Lift, não turbofish Rust.
- Argumentos explícitos não contornam as regras de genéricos: recursos/`DbValue`/closures não instanciam `T`, e funções genéricas como valores/callbacks continuam sem suporte.

## Limites

- Sem argumentos explícitos em métodos, construtores, closures, imports FFI ou nos próprios containers/enums genéricos.
- Sem listas parciais/inferidas (`f::<_, int>`), defaults, constraints ou parâmetros usados só no retorno.
- Funções genéricas sobre parâmetros de container (`function f<T>(box: Box<T>)`) ainda não são suportadas.

Testes: `crates/lift_sema/tests/explicit_type_arguments_contract.rs`; exemplo [`examples/explicit-type-arguments.lift`](../examples/explicit-type-arguments.lift).
