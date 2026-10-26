# Métodos, closures e templates v1

Rust/Cargo **1.94.1+** é obrigatório para compilador, core embutido e projetos
gerados. Este é um subconjunto definido do Truxt, sem compatibilidade completa
com TypeScript. Execute `cargo run -p truxt_cli -- run examples/methods-closures-templates.truxt`.

## Métodos nominais

```truxt
interface Counter { value: int }
impl Counter {
    add(n: int): int { this.value = this.value + n; return this.value; }
    snapshot(): Counter { return this; }
}
const c: Counter = { value: 1 };
console.log(c.add(c.value));
```

`impl` exige uma interface nominal existente. Métodos aceitam `nome(...)` ou
`function nome(...)` e seguem as regras de tipos/retorno das funções comuns.
`this` é o primeiro parâmetro implícito; reatribuí-lo ou declarar parâmetro
explícito `this` é erro. A resolução usa o tipo nominal do receiver, sem métodos
como propriedades/valores, métodos estáticos, overloads, traits, herança ou
dispatch dinâmico. Duplicatas são erro. Extrair `const f = c.add` é rejeitado;
acesso comum a campo continua sendo acesso a campo.

A semântica cria uma função interna e insere `this` como primeiro argumento HIR
antes da análise comum de ownership/aliasing. Leitura empresta o receiver;
mutação o empresta mutavelmente. Retornar `this` clona um snapshot owned, mantendo
o efeito da mutação no receiver do chamador. Outros parâmetros preservam as
regras de valor/cópia do Truxt, incluindo clone para parâmetros owned não-Copy.
O receiver é avaliado antes dos argumentos; staging existente materializa
leituras/efeitos conflitantes antes do empréstimo final. `c.add(c.value)` e
`c.add(c.add(1))` são compilados no E2E; dois argumentos mutáveis simultâneos com
raiz `c` são rejeitados. Receiver temporário mutável e projeções/aliasing fora
dos padrões suportados recebem os diagnósticos existentes. Resumos de mutação
propagam até ponto fixo. Receiver que é parâmetro nominal direto seleciona o
resumo exato; para locais/projeções sem tipo disponível no walker de resumos, a
análise une conservadoramente os métodos de mesmo nome, podendo exigir empréstimo
mutável mesmo quando outro tipo nominal possui esse método.

## Arrow closures locais

```truxt
let text = "antes";
const snapshot = (): string => text;
text = "depois";
console.log(snapshot()); // antes
const plus = (x: int): int => x + 1;
console.log(plus(2));
```

Closures exigem parênteses e tipos explícitos dos parâmetros e retorno. O corpo
pode ser expressão ou bloco; blocos não-void precisam retornar em todos os
caminhos. Podem inicializar diretamente um `const` local ou ser chamadas
imediatamente. Closures nomeadas admitem chamadas repetidas. Parâmetros são
valores owned (argumentos não-Copy são clonados quando necessário); projeções de
parâmetros e variáveis comuns podem ser mutadas, preservando a proibição de
reatribuição direta de parâmetros.

Nomes locais livres são resolvidos lexicalmente na construção. Captures são
**snapshots owned independentes**: cópia de primitivos e clone explícito de
strings, structs, arrays, Option e Result. Isso difere intencionalmente dos
bindings vivos capturados pelo TypeScript. Mudanças posteriores no binding ou
nas projeções externas não aparecem na closure. Captures não admitem atribuição
nem mutação, inclusive via método/função mutante. `this` dentro de arrow é um
snapshot do receiver externo. Funções globais continuam chamadas diretamente;
não se tornam valores capturados. Capturar outra closure, recursão pelo nome
local, reatribuir/copiar closures, armazená-las em campos/arrays, retorná-las,
recebê-las como callbacks, tipos de função em anotações e closures FFI são
rejeitados. Adapters map/filter/find com callback continuam exclusivos de Rust.
Closures aninhadas podem capturar valores comuns.

O HIR registra assinatura, corpo tipado, IDs dos parâmetros e ID próprio de cada
capture com inicializador owned/CloneMode explícito. Rust emite `move` closure.
A checagem evita consumir captures no corpo, preservando chamadas repetidas.
Liveness observa inicializadores na construção e nunca visita o corpo adiado no
CFG externo; o corpo mantém clones conservadores sem empréstimo do frame externo
nem moves de último uso calculados para o chamador.

## Template strings

```truxt
console.log(`valor ${c.value}, aninhado ${`ok ${true}`}`);
console.log(`literal \${text}, crase \` e quebra\n`);
```

Crases delimitam templates multilinha. `${expressão}` aceita expressões comuns,
incluindo templates aninhados, strings com chaves, comentários, objetos e
chamadas. O lexer contextual balanceia chaves e preserva spans semiabertos de
bytes UTF-8 e SourceId, inclusive em templates aninhados. Templates/interpolações
abertos são erros léxicos; interpolação vazia ou com tokens restantes é erro de
parser. Tagged templates não são suportados.

Apenas int, number, boolean e string interpolam. Cada expressão executa uma vez,
da esquerda para direita, convertendo imediatamente para String owned antes da
próxima interpolação. Em `${c.value}/${c.add(1)}/${c.value}`, a primeira leitura
ocorre antes da mutação e a última depois. O HIR usa chamadas reservadas de
stringify com empréstimo e concatenação ordenada de Strings; Rust usa `format!`
com formato constante, evitando interpretação das chaves da fonte como formato.
Formatação segue Rust Display, sem prometer toda a formatação numérica JS/TS.
Escapes seguem strings comuns: `\\`, `\"`, `\'`, `\n`, `\r`, `\t`, `\0`; `\`` e
`\$` protegem crase e início de interpolação. Escape desconhecido remove a
barra. Escapes Unicode, tagged raw e continuação de linha JS não são
implementados; caracteres Unicode podem ser escritos diretamente.

Testes semânticos cobrem assinaturas, captures no HIR, aliasing inválido,
captures/escapes/containers rejeitados, aninhamento léxico e diagnósticos UTF-8.
A integração da CLI verifica o Rust gerado e executa o exemplo combinado,
comparando sua saída. Isso valida as combinações exercitadas, sem provar borrow
checking irrestrito nem compatibilidade completa com TypeScript.
