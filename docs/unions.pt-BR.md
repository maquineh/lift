# Tipos union

[English](unions.md) · [Guia de uso](../README.pt-BR.md)

`type A = B | C;` declara uma união nominal sobre tipos nominais existentes. Ela é desaçucarada para um enum com uma variante por membro, então construção e matching reutilizam o contrato de enum.

```lift
interface Circle { radius: number }
interface Square { side: number }

type Shape = Circle | Square;

function area(shape: Shape): number {
    match shape {
        Shape.Circle(circle) => { return 3.14 * circle.radius * circle.radius; }
        Shape.Square(square) => { return square.side * square.side; }
    }
}

const shapes: Shape[] = [
    Shape.Circle({ radius: 2.0 }),
    Shape.Square({ side: 3.0 }),
];
```

## Declaração

```text
type Name = Member | Member;
```

- Cada membro precisa ser um **tipo nominal nomeado**: interface, enum ou outra union. Primitivos, arrays, `Option`, `Result`, closures e tipos estruturais inline são rejeitados como membros; envolva em uma interface quando necessário.
- A união vira um enum nominal `Name`; cada membro vira uma variante posicional nomeada pelo tipo do membro com um payload desse tipo. A união precisa de ao menos um membro, e membros duplicados são rejeitados.
- O nome participa do mesmo namespace de tipos que interfaces/enums: declaração duplicada é rejeitada. Referências futuras são permitidas.
- `type` é uma forma de declaração, não um alias geral. Não há alias para tipo não-union nem `A | B` inline em parâmetro/campo.

## Construção e valores

```lift
const circle = Shape.Circle({ radius: 2.0 });
const square = Shape.Square({ side: 3.0 });
```

- A construção de variante segue as regras de enum: o único payload é checado contra o tipo do membro e materializado como owned. Como o payload é uma struct, um object literal é aceito diretamente.
- Valores de união são valores de enum: `Clone`, `Debug`, `PartialEq` estrutural, utilizáveis em arrays, parâmetros, retornos e payloads de `Option`/`Result`.
- Recursão é suportada quando passa por campo/array: por exemplo `interface Node { children: Tree[] } type Tree = Node;`. Payloads que alcançam enum recebem `Box` automático; arrays já usam `Vec`.
- Uniões podem ser campos de interface, payloads de enum, parâmetros/retornos e declarações de módulo. FFI ainda rejeita tipos union/enum.

## Matching

Um match de união é exaustivo sobre as variantes dos membros:

```lift
match shape {
    Shape.Circle(circle) => { console.log(circle.radius); }
    Shape.Square(square) => { console.log(square.side); }
}
```

Bindings são posicionais (um payload) e imutáveis, com campos de struct mutáveis quando necessário. Variante ausente/duplicada/desconhecida é `sema.InvalidMatch`. `return`, `throw`, `?`, `break` e `continue` funcionam como nos demais braços de enum.

## Limites

- Sem membro primitivo/array/Option/Result; introduza uma interface wrapper.
- Sem tipo union inline, alias para não-union ou construção anônima de união.
- Sem inferência de discriminante/narrowing: matching é a única forma de acessar um membro.
- Sem unions genéricas, métodos/`impl` em uniões ou unions FFI; uniões recursivas usam as mesmas regras de `Box` automático dos enums.
- Enums/unions continuam nominais; não há igualdade estrutural ou coerção entre nomes de união diferentes.

Testes de parser, semântica, codegen e CLI cobrem declaração/validação de membros, construção por object literal, match exaustivo, membros enum, recursão e execução nativa de [`examples/unions.lift`](../examples/unions.lift).
