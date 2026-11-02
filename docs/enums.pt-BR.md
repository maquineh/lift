# Enums

[English](enums.md) · [Guia de uso](../README.pt-BR.md)

Enums Lift são tagged unions nominais com payloads posicionais. Variantes são construídas com `Enum.Variant(...)` e casadas exaustivamente.

```lift
enum Shape {
    Circle(number),
    Rect(number, number),
    Empty,
}

function area(shape: Shape): number {
    match shape {
        Shape.Circle(radius) => { return 3.14 * radius * radius; }
        Shape.Rect(width, height) => { return width * height; }
        Shape.Empty => { return 0.0; }
    }
}

const shapes: Shape[] = [Shape.Circle(2.0), Shape.Rect(2.0, 3.0), Shape.Empty];
```

## Declaração

```text
enum Name {
    Variant,
    Variant(Type, Type),
    Variant { field: Type, other: Type },
}
```

- Variantes têm nome e nenhum payload, **payloads posicionais** ou **campos nomeados** (formas mutuamente exclusivas). Vírgulas entre variantes/campos e a vírgula final são opcionais.
- Tipos de payload são tipos de valor Lift comuns: primitivos, `string`, arrays, interfaces, outros enums, `Option` e `Result`. Payloads recursivos são embrulhados automaticamente em `Box` pelo compilador; recursos opacos, closures e `DbValue` continuam rejeitados.
- Declarações de enum podem vir antes ou depois das funções que as usam. Módulos de arquivo compartilham o namespace flat, então um enum declarado em módulo importado fica visível no pacote; enums qualificados por pacote seguem as mesmas regras dos demais tipos locais.
- Um enum precisa de ao menos uma variante. Nomes duplicados (contra interfaces/funções) e variantes duplicadas são rejeitados.

## Construção e semântica de valor

```lift
const empty = Shape.Empty;
const circle = Shape.Circle(2.0);
const named = Shape.NamedCircle({ radius: 2.0 });
```

- Variantes posicionais usam `Enum.Variant(args...)`; variantes com campos nomeados usam um object literal `Enum.Variant({ field: value, ... })`. Contagem, nomes de campo, campos ausentes/desconhecidos/duplicados e tipo de cada valor são checados antes do codegen; valores são materializados como owned.
- Variante com payload precisa ser chamada; variante sem payload é usada diretamente. Parênteses vazios são aceitos para variante sem payload.
- Enums são tipos de valor: podem ficar em arrays, ser passados/retornados e servir de payload de `Option`/`Result`. Atribuições/cópias seguem as regras normais de clone de Lift (o enum Rust deriva `Clone, Debug, PartialEq`).
- Igualdade é estrutural: `a == b` / `a != b` comparam variante e payloads. Ordem (`<`, `>`) não é suportada.
- Enums podem ser campos de interface (o campo é inline); parâmetros/retornos FFI ainda rejeitam enums. Tipos recursivos são suportados via campos `Box` automáticos, inclusive ciclos struct↔enum.

## Recursão e campos de interface\n\nPayloads que alcançam um enum (direto ou via campos de interface) recebem `Box` automático no enum Rust gerado; valores Lift continuam sem Box, e construção/matching tratam a indireção. Isso permite `enum List { Nil, Cons(int, List) }`, enums mutuamente recursivos e ciclos como um payload de enum contendo uma interface com campo enum. Interfaces também podem ter campos enum diretamente, e enums podem ter payloads de interface.\n\n## Matching

`match` exige um braço por variante (veja [matching](match.pt-BR.md)):

```lift
match shape {
    Shape.Circle(radius) => { console.log(radius); }
    Shape.Rect(width, height) => { console.log(width + height); }
    Shape.Empty => { console.log(0); }
}
```

- Patterns são qualificados (`Shape.Circle(...)`). Variantes posicionais ligam argumentos em ordem; variantes com campos nomeados ligam na ordem de declaração dos campos usando a mesma sintaxe posicional (por exemplo, `Shape.Circle(radius)` para `{ radius: number }`). A contagem de bindings precisa coincidir com a de payloads.
- Bindings são valores imutáveis com escopo no braço; campos podem ser mutados quando o payload é struct (o pattern Rust gerado usa `mut`).
- O nome do enum no pattern precisa coincidir com o do scrutinee. Braço ausente, variante duplicada, variante desconhecida e pattern de outro enum são `sema.InvalidMatch`.
- Um match é exaustivo quando toda variante declarada tem exatamente um braço. Não há braço wildcard `_`, guarda ou pattern aninhado neste incremento.
- O scrutinee é avaliado uma vez e clonado quando é um place, então o enum original continua utilizável; payloads são valores owned desse clone. Match de enum é statement, não expressão.
- `return`, `throw`, `?`, `break` e `continue` funcionam nos braços com os mesmos alvos dos demais matches.

## Limites

- Sem discriminantes explícitos/valores numéricos, métodos/`impl` em enums e variantes genéricas. Recursão e campos de interface são suportados via indireção `Box` inserida pelo compilador; não há tipo `Box` explícito em Lift.
- Enums não podem aparecer em campos de interface, layouts externos, payloads de callback via FFI ou posições de `DbValue`.
- A sintaxe de union TypeScript `type A = B | C` não está implementada; tagged unions são expressas com este `enum` nominal.
- Sem wildcard/guarda/pattern aninhado, sem expressão `match` de enum e sem `switch`.
- Igualdade exige tipos de enum idênticos; não há coerção entre enums.

Testes de parser, semântica, codegen e CLI cobrem declarações, construção de payload posicional e nomeado, recursão/campos de interface, match exaustivo, variantes/campos duplicados/ausentes, limites, igualdade, semântica de valor e execução nativa de [`examples/enums.lift`](../examples/enums.lift), [`examples/enum-fields.lift`](../examples/enum-fields.lift) e [`examples/recursive-enums.lift`](../examples/recursive-enums.lift).
