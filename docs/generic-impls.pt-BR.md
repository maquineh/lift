# Métodos de impl genérico

[English](generic-impls.md) · [Guia de uso](../README.pt-BR.md)

`impl Box<T>` declara métodos uma vez para uma interface genérica; o compilador cria um impl concreto para cada aplicação descoberta no programa (`Box<int>`, `Box<string>`), substituindo `T` nas assinaturas, corpos e anotações locais.

```lift
interface Box<T> { value: T }

impl Box<T> {
    get(): T { return this.value; }
    set(value: T): void { this.value = value; }
}

let integer: Box<int> = { value: 1 };
console.log(integer.get()); // 1
integer.set(2);
console.log(integer.get()); // 2

let text: Box<string> = { value: "hi" };
console.log(text.get()); // hi
```

## Como funciona

- Declarações de impl genérico são coletadas antes do monomorfizador de containers.
- Todas as aplicações concretas da interface alvo são descobertas em anotações, parâmetros, retornos, declarações locais e tipos de closure.
- Para cada aplicação, os métodos são clonados, `T` é substituído e um `impl Box<int>` concreto é gerado. O lowering normal de impl adiciona `this: Box<int>` e mangleia os nomes (`$method:Box<int>:get`).
- Chamadas como `integer.get()` resolvem pelo tipo concreto do receptor. Mutações em `this`, modos de passagem e regras de recursos usam a maquinaria existente de métodos.
- O template `impl Box<T>` é removido após a especialização; não é chamável como tipo bruto.

## Limites

- O alvo precisa ser uma interface genérica com aridade correspondente; impls genéricos para enums/unions, layouts externos ou interfaces desconhecidas não são suportados.
- A especialização é dirigida por aplicações descobertas: um impl sem uso concreto não produz métodos. Nomes de método são internos/mangleados.
- Sem métodos genéricos aninhados no impl (parâmetros de tipo no método não são especializados), sem impls genéricos em FFI e sem API nativa de Map ainda.
- A mesma aplicação encontrada em várias anotações gera um único impl (deduplicado pelo nome canônico).

Testes: `crates/lift_sema/tests/generic_impls_contract.rs`; exemplo [`examples/generic-impls.lift`](../examples/generic-impls.lift).
