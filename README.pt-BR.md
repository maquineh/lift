# Lift

`lift lint <entrada.lift>` emite avisos sem gerar Rust (`--deny-warnings` no CI).
`lift lsp` e a [extensão local VS Code](editors/vscode/README.pt-BR.md) diagnosticam
arquivos não salvos. Consulte os contratos e limites da
[API de projeto e tooling](docs/project-analysis-and-lsp.pt-BR.md).

Rust/Cargo **1.94.1+** (MSRV: `1.94.1`).

Lift é uma linguagem pequena e estaticamente tipada que compila uma entrada `.lift` e seus módulos explícitos para Rust, usando Cargo para verificar, compilar ou executar.

> **Escopo atual:** programas tipados com módulos relativos, pacotes locais Forklift com exports explícitos de funções, biblioteca padrão embutida, imports Rust seguros, console e I/O UTF-8 baseado em Result. Os executáveis podem ser movidos independentemente do checkout. Rust, Cargo e um linker do host continuam necessários para compilar programas.

Lift busca ser familiar para quem usa TypeScript dentro de um subconjunto documentado; não é uma implementação completa de TypeScript. Cada recurso novo deve definir a semântica Lift e gerar Rust válido, considerando ownership, borrowing e segurança do Rust.

## Requisitos

- Rust **1.94.1+**, com `rustc` e `cargo` disponíveis no `PATH`.
- Linker funcional para o target hospedeiro do Rust.
- Acesso à rede quando o programa importar uma crate registry que ainda não esteja no cache do Cargo.

A matriz de CI está configurada para Windows e Ubuntu com Rust `stable` e MSRV 1.94.1. PostgreSQL 17 tem um job de integração separado. macOS e compilação cruzada ainda não fazem parte da matriz suportada; compile e execute para o host.

## Compile a CLI a partir deste checkout

Na raiz do repositório:

```sh
cargo build --release -p lift_cli
```

O executável será gravado em `target/release/lift` (ou `lift.exe` no Windows). As fontes da stdlib são incorporadas durante sua compilação; o executável não precisa mais do checkout original.

O mesmo build produz `forklift`, CLI de pacotes locais. Inicie um projeto
manifestado com `forklift init my-app` e use seus comandos resolve/check/build/run.
Veja [pacotes locais Forklift](docs/forklift.pt-BR.md) para `lift.toml`,
`import * as math from "math"`, `export function` e a distinção de crates Rust.
Forklift resolve apenas paths locais nesta etapa.

## Instalação ou pacote portátil

Instale a partir do checkout no diretório de executáveis do Cargo:

```sh
cargo install --path crates/lift_cli --locked
lift --version
```

Também é possível copiar o executável release para um diretório no `PATH`. Somente o executável é necessário do Lift; Rust/Cargo e o linker do host continuam pré-requisitos. No PowerShell, crie um ZIP com executável, guias bilíngues e exemplos:

```powershell
./scripts/package.ps1
```

Extraia `dist/lift-<versão>-<host>.zip` em qualquer pasta e adicione essa pasta ao `PATH`, ou chame o executável pelo caminho. O script compila para o host atual do Rust. Ele não instala Rust nem altera o `PATH`.

## Início rápido

Crie um arquivo inicial e execute-o:

```sh
cargo run -p lift_cli -- init ola
```

Edite `ola/main.lift`:

```lift
console.log("Olá, Lift!");
```

Execute o programa:

```sh
cargo run -p lift_cli -- run ola/main.lift
```

Com o executável compilado no `PATH`, os comandos equivalentes são `lift init ola` e `lift run ola/main.lift`.

## Comandos da CLI

| Comando | Comportamento |
| --- | --- |
| `lift init [diretório]` | Cria `main.lift`; recusa sobrescrever um arquivo existente. |
| `lift check arquivo.lift` | Analisa Lift, gera Rust e executa `cargo check`. |
| `lift build arquivo.lift` | Compila em modo release e copia o executável para perto do arquivo fonte. |
| `lift run arquivo.lift [-- args...]` | Compila em modo release e executa o binário, encaminhando os argumentos. |

Os projetos e artefatos Cargo ficam em `.lift_cache/<nome-da-entrada>/`, ao lado da entrada. A CLI extrai o core em `lift_core/` nesse projeto e usa dependência Cargo relativa. `main.rs`, `Cargo.toml` e core só são gravados quando seus bytes mudam. Alterações no core extraído são sobrescritas no próximo comando. Isso evita reescritas desnecessárias; carregamento de módulos, análise, codegen e Cargo continuam executando a cada chamada, portanto não é cache completo por chave de conteúdo.

## Módulos de arquivos

Importe outra fonte Lift no topo:

```lift
import module "lib/math.lift";
console.log(double(21));
```

`lib/math.lift` pode declarar a função:

```lift
function double(value: int): int { return value * 2; }
```

Caminhos são relativos ao arquivo importador e exigem `.lift`. Imports são recursivos; cada caminho canônico entra uma vez, permitindo repetições e ciclos. Somente a entrada pode conter statements executáveis de topo. Módulos importados podem declarar funções, interfaces, imports Rust FFI e outros módulos. Módulos de arquivo continuam flat e compartilham namespace dentro do pacote; duplicatas geram erro com labels nos dois arquivos. Sem manifesto permanece o namespace global legado. Projetos manifestados também suportam aliases de pacotes e funções exportadas pelo [Forklift](docs/forklift.pt-BR.md). A sintaxe Rust `import function ... from ...` não mudou.

Execute `lift run examples/modules/main.lift` para imports relativos/transitivos, FFI, tipos compartilhados e ciclos incluindo a entrada. Consulte o [contrato de módulos](docs/modules.pt-BR.md) para identidade de fonte e limites exatos.

## Diagnósticos de fonte

Erros léxicos, sintáticos e semânticos usam relatórios Ariadne com caminho da fonte, mensagem, trecho, label principal e label relacionado quando o compilador fornece span secundário. Spans de bytes UTF-8 são mostrados na posição de caractere correta, incluindo spans EOF vazios. Erros de fonte mantêm código de saída `1`.

Cores só são ativadas quando stderr é um terminal interativo e podem ser desligadas com `NO_COLOR`; saídas redirecionadas não incluem cores ANSI. Diagnósticos Cargo/rustc continuam sendo os relatórios do Cargo sobre o Rust gerado, sem remapeamento para Lift. Consulte [diagnósticos e arquivos gerados](docs/diagnostics-and-build.pt-BR.md) para o contrato da implementação.

## Escopo da linguagem e da interoperabilidade

A implementação atual oferece funções tipadas, valores primitivos (`int`, `number`, `boolean`, `string`), arrays, interfaces/literais de objetos, fluxo de controle e os padrões de ownership cobertos pelos exemplos e testes. A linguagem não insere ponto e vírgula a partir de quebras de linha. Declarações de import ficam no topo e exigem ponto e vírgula final.

Imports Rust declaram uma assinatura Lift explícita e um caminho Rust:

```lift
import function concat(left: string, right: string): string from "lift_core::string::concat";
console.log(concat("Olá, ", "Lift!"));
```

O FFI aceita funções Rust livres e seguras com escalares, arrays de um nível, structs externas, valores opcionais diretos e parâmetros/retornos `Result`. Parâmetros `string` são passados como `&str`, e retornos string como `String` owned; parâmetros array são slices emprestados e retornos array são `Vec<T>` owned; parâmetros de struct externa são emprestados e retornos são owned. `T?` vira `Option<&str>`, `Option<&[T]>`, `Option<&P>` ou `Option<escalar>` em parâmetros e o option owned em retornos. `void` corresponde a `()`. Um import também pode receber e retornar `Result<T, E>`: parâmetros são emprestados (`&Result<...>`), payloads usam o mapeamento owned e podem conter um `Option` direto, e `Result`/`Option<Result>` aninhados continuam fora. Parâmetro de tipo função é um callback (`&dyn Fn` com payloads owned), e retorno de tipo função é `Box<dyn Fn>` convertido para `Rc` pelo wrapper. Imports registry usam versão exata `MAJOR.MINOR.PATCH`. O wrapper Rust deixa `rustc` verificar caminho e assinatura.

Uma struct Rust é declarada com `import interface`, que mapeia cada campo Lift para um campo Rust público e exige `Clone + Debug + PartialEq`; o código gerado afirma o layout, então `rustc` rejeita campo ausente, privado, renomeado ou com tipo diferente:

```lift
import interface IntRange {
    start: int,
    end: int,
} from "lift_core::interop::IntRange";
import function int_range(start: int, end: int): IntRange from "lift_core::interop::int_range";

const range = int_range(3, 8);
console.log(range.start);
```

Somente tipos declarados com `import interface` cruzam a FFI; structs locais Lift, Option aninhado em arrays/Result, arrays aninhados, `unsafe` e raw identifiers não são suportados. Consulte [structs externas e Option](docs/ffi-structs-and-options.pt-BR.md) para o mapeamento exato, as checagens de layout e os limites.

`console.log` aceita uma expressão de `int`, `number`, `boolean` ou `string` e a escreve com uma quebra de linha. Atualmente, falhas ao escrever em stdout causam panic.

A crate Rust-native `lift_core` inclui helpers de console, arrays e strings. Os adaptadores compatíveis com FFI ficam em `lift_core::interop`:

```lift
import function tamanho(values: int[]): int from "lift_core::interop::array_len";
import function trim(value: string): string from "lift_core::interop::string_trim";
import function contem(values: int[], value: int): boolean from "lift_core::interop::array_contains_int";
```

Os adaptadores também oferecem `string_len` e `array_contains_number`, `array_contains_boolean` e `array_contains_string`, além de adaptadores de callback como `apply_twice` e `apply_text`. Arrays Lift também têm métodos embutidos `map`/`filter`/`find`/`reduce` (veja [métodos de array](docs/array-methods.pt-BR.md)); os adaptadores Rust `map`/`filter`/`find` continuam disponíveis para chamadores Rust. Consulte o [guia da biblioteca core](stdlib/lift_core/README.pt-BR.md) para a API completa.

## Tipos de função e callbacks

Um tipo de função é escrito `(T) => R`. Closures e funções nomeadas podem ser armazenadas, copiadas, passadas, retornadas, capturadas por outras closures e passadas a imports Rust como callbacks:

```lift
function double(value: int): int { return value * 2; }
function apply(value: int, transform: (int) => int): int { return transform(value); }
function make_adder(amount: int): (int) => int {
    return (value: int): int => value + amount;
}

const stored: (int) => int = double;
console.log(apply(3, stored));   // 6
console.log(make_adder(2)(10));  // 12
```

Valores de função geram `Rc<dyn Fn(owned...) -> owned>`, então cópias clonam o `Rc` e callbacks passam com clone barato. Captures continuam snapshots somente leitura. Um parâmetro de import com tipo de função vira `&dyn Fn(...)`, e um callback retornado é `Box<dyn Fn(...)>` convertido para `Rc`. Valores de função são rejeitados em campos de struct, arrays, `Option` e `Result`; métodos não são de primeira classe, e callbacks com capture mutável ou recursos opacos são rejeitados. Consulte [tipos de função e callbacks](docs/higher-order.pt-BR.md) para o contrato completo e o mapeamento FFI.

## Laços

`for (const value of values)` itera um snapshot owned de um array, e `while` mantém o contrato booleano. `break` e `continue` têm como alvo o laço mais interno, inclusive dentro de `try`/`catch`:

```lift
function first_positive(values: int[]): int? {
    for (const value of values) {
        if value > 0 { return option.some(value); }
    }
    return null;
}

for (let value of [1, 2]) {
    if value == 2 { break; }
    console.log(value);
}
```

Um binding `const` do laço é somente leitura; `let` permite reatribuir o binding sem mutar o array de origem. O array é clonado uma vez antes da iteração, então mutações posteriores não são observadas, e cada elemento é um valor owned. `break`/`continue` são rejeitados fora de laço e dentro de closures. Um laço nunca conta como retorno da função, então a função continua precisando de `return` final. Não há `for` estilo C, `for...in` sobre índices, labels, ranges ou protocolo de iterator. Veja [laços e controle de laço](docs/loops.pt-BR.md).

## Métodos de array

Arrays expõem métodos higher-order embutidos que recebem callbacks Lift: `map`, `filter`, `find`, `some`, `every`, `forEach` e `reduce`.

```lift
function double(value: int): int { return value * 2; }

const values = [1, 2, 3, 4];
const doubled = values.map(double);                 // int[]
const evens = values.filter((value: int): boolean => value % 2 == 0);
const first = values.find((value: int): boolean => value > 2); // int?
const total = values.reduce(0, (accumulator: int, value: int): int => accumulator + value);
```

O receptor é copiado uma vez, então o array de origem nunca é consumido nem mutado; cada elemento chega ao callback como valor owned. `map` e `filter` retornam novos arrays, `find` retorna `T?`, `some`/`every` fazem curto-circuito, `forEach` retorna `void`, e `reduce` é um fold à esquerda cujo tipo do acumulador vem do valor inicial. Callbacks podem ser closures ou funções nomeadas, e chamadas encadeiam. Método desconhecido, callback com tipo de parâmetro/resultado errado, número errado de argumentos, `find` em `T?[]` ou receptor que não é array são rejeitados antes do codegen. Não há `flatMap`, `sort`, parâmetro de índice, iteradores lazy ou métodos mutantes como `push`. Veja [métodos de array](docs/array-methods.pt-BR.md).

## Match de Option, Result e enums

`match` executa exatamente um braço para um `Option<T>` ou `Result<T, E>`, e as duas variantes são obrigatórias:

```lift
function describe(value: int?): string {
    match value {
        some(present) => { return `some ${present}`; }
        none => { return "none"; }
    }
}

function describe_result(value: Result<int, string>): string {
    match value {
        ok(number) => { return `ok ${number}`; }
        err(error) => { return `err ${error}`; }
    }
}
```

O scrutinee é avaliado uma vez e clonado quando é um place, então o valor original continua utilizável. `some`/`err` ligam um payload owned; `ok()` é a forma de sucesso `void`. Bindings de braço são imutáveis (campos podem ser mutados), e braço ausente ou duplicado é rejeitado. Um match garante retorno quando todos os braços saem, e `break`/`continue` dentro de um braço miram o laço externo. Match de `Result` com recurso opaco é rejeitado; use `?`/`unwrap`. Enums nominais são casados com patterns de variante qualificados; wildcards, guardas, patterns aninhados e tipos union TypeScript não foram implementados. Veja [matching](docs/match.pt-BR.md).

## Enums

Enums são tagged unions nominais com payloads posicionais:

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

Uma variante é construída com `Shape.Circle(2.0)` e uma sem payload com `Shape.Empty`; contagem e tipos dos payloads são checados. Enums são tipos de valor com igualdade estrutural, utilizáveis em arrays, parâmetros, retornos e payloads de `Option`/`Result`. Match exige exatamente um braço por variante e liga payloads posicionalmente; variante ausente/duplicada/desconhecida é rejeitada. Enums não podem ser campos de interface, parâmetros/retornos FFI, nem conter outros enums/payloads recursivos ainda; não há variantes com campos nomeados, discriminantes explícitos ou unions TypeScript `type A = B | C`. Veja [enums](docs/enums.pt-BR.md).

## Valores opcionais

Use `T?`, `null` contextual e `option.some(value)` explícito:

```lift
const absent: int? = null;
const present = option.some(7);
console.log(absent ?? present ?? 9);
```

`??` avalia a esquerda uma vez e o fallback somente na ausência. `T? ?? T` retorna `T`; `T? ?? T?` continua opcional. `option.is_some` testa e `option.unwrap` clona o payload presente; unwrap na ausência causa panic. `int?[]` é array de opcionais; `int[]?` é array opcional. Campo `T?` continua obrigatório; campo `nickname?: string` pode ser omitido, e a omissão vira `None`. Dentro de `if value != null`, `if value == null { return; }`, cadeias `else if` e corpos de `while option.is_some(value)`, um `const` ou parâmetro imutável de tipo `T?` estreita para `T`; valores estreitados são clones somente leitura, então mutação e narrowing de `let` são rejeitados e a própria condição não é estreitada. Não há lift implícito, ausência `undefined`, optional chaining ou narrowing de `Result`. Um parâmetro/retorno `T?` direto agora é suportado em imports Rust; Option aninhado em arrays/Result não é. Consulte o [contrato exato](docs/option.pt-BR.md) e o [mapeamento struct/Option na FFI](docs/ffi-structs-and-options.pt-BR.md), incluindo limites de inferência, narrowing e parênteses entre operadores.

## Entrada e arquivos

`Result<T, E>` é um tipo especial disponível em anotações e assinaturas de funções Lift. Use imports explícitos para argumentos, entrada padrão e arquivos UTF-8:

```lift
import function arguments(): Result<string[], string> from "lift_core::io::arguments";
import function read_stdin(): Result<string, string> from "lift_core::io::read_stdin";
import function read_file(path: string): Result<string, string> from "lift_core::io::read_file";
import function write_file(path: string, contents: string): Result<void, string> from "lift_core::io::write_file";
```

`arguments()` retorna os argumentos sem o nome do executável, preservando ordem e espaços. Use `--` após a fonte para encaminhar flags: `lift run app.lift -- --flag "a b"`. `read_stdin()` lê até EOF, incluindo quebras de linha. `read_file()` exige UTF-8; `write_file()` cria ou trunca o arquivo sem criar diretórios pais. Caminhos relativos usam o diretório de trabalho do chamador, preservado por `lift run`.

Falhas de I/O e UTF-8 inválido tornam-se strings de erro em vez de panic. Trate o resultado com um ramo:

```lift
const contents = read_file("input.txt");
if result.is_ok(contents) {
    console.log(result.unwrap(contents));
} else {
    console.log(result.unwrap_err(contents));
}
```

A inspeção de Result empresta o argumento; unwrap clona o payload para preservar o Result original. Unwrap na variante errada causa panic. Valores Result podem ser construídos com `result.ok(value)`/`result.err(error)` (contextuais), propagados com `?` postfix, lançados com `throw` e tratados com `try`/`catch` tipado; tipos de erro precisam coincidir exatamente e não há conversão `From` implícita. Generics gerais, matching geral além de `Option`/`Result`, `finally` e múltiplos catches não foram implementados. Consulte o [contrato Result/I/O](docs/result-and-io.pt-BR.md) e [erros e propagação](docs/errors.pt-BR.md) para o escopo exato. Exemplo que lê entrada canalizada e grava um arquivo:

```powershell
"Olá da entrada padrão" | lift run examples/io.lift -- output.txt exemplo
```

O arquivo resultante contém `Olá da entrada padrão|exemplo`. Ao executar interativamente, o programa continua lendo stdin até EOF.

## Limitações atuais

- Uma entrada executável, com módulos flat legados ou pacotes locais `lift.toml`. Forklift suporta dependências path e funções públicas de primitivos/containers; sem registry Lift, dependências remotas, tipos customizados públicos ou reexports nesta etapa.
- Não há cache completo por chave de conteúdo. Arquivos gerados idênticos são preservados e Cargo reutiliza seus artefatos; análise/codegen continuam executando a cada chamada.
- A análise de ownership continua conservadora e rejeita alguns padrões complexos de aliasing. Uma passagem privada CFG/liveness remove somente clones comprovadamente de último uso de locais owned diretos; valores emprestados, projeções, usos repetidos no statement e argumentos `ByClone` mantêm cópias. Consulte o [contrato da otimização](docs/liveness.pt-BR.md).
- Métodos nominais/`impl`, arrow closures com snapshot (incluindo callbacks de primeira classe) e interpolação de primitivos estão implementados; veja [contratos e limites](docs/methods-closures-templates.pt-BR.md) e [tipos de função e callbacks](docs/higher-order.pt-BR.md). Unions continuam pendentes. Imports Rust aceitam structs externas, Option direto e parâmetros `Result` emprestados com payload opcional direto; campos opcionais aceitam `?:`/omissão, mas o narrowing cobre apenas bindings imutáveis simples, continuações de guarda, laços e captures de closure.
- Valores Result usam construtores contextuais `result.ok`/`result.err`, propagação `?`/`throw` com correspondência exata de tipo de erro e `try`/`catch` tipado; `finally`, múltiplos catches, matching geral além de `Option`/`Result` e conversão automática `From` não foram implementados.
- Laços cobrem `while` e `for...of` sobre arrays com `break`/`continue`; `for` estilo C, labels, ranges, protocolo de iterator e expressões de laço não foram implementados.
- Métodos de array cobrem `map`, `filter`, `find`, `some`, `every`, `forEach` e `reduce`; `flatMap`, `sort`, parâmetro de índice, iteradores lazy e métodos mutantes como `push` não foram implementados.
- Matching cobre `Option`/`Result` e enums nominais; wildcards, guardas, patterns aninhados e tipos union TypeScript não foram implementados.
- Enums usam apenas payloads posicionais; variantes com campos nomeados, enums recursivos, enums em interfaces/FFI, discriminantes explícitos e métodos em enums não foram implementados.
- O [serviço local de notas](examples/notes-service/README.pt-BR.md) declara DTOs, handlers síncronos, rotas e SQL em Lift. A [API de banco](stdlib/lift_db/README.pt-BR.md) oferece pools SQLite/PostgreSQL configuráveis, execute/query parametrizado, resultados materializados e transações owned. O servidor possui o pool e os handlers o emprestam.

O serviço exercita os contratos documentados de SQL/escalares e HTTP estático. Consulte o [plano de compatibilidade de banco](docs/database-compatibility.pt-BR.md) para a matriz de backends/versões e o [contrato de callbacks/HTTP](docs/callbacks-and-web-contract.pt-BR.md) para limites do adaptador.

## Exemplos e testes

- `examples/fibonacci.lift` — funções recursivas e fluxo de controle.
- `examples/ownership.lift` — mutação de agregados, empréstimos e valores de retorno owned.
- `examples/interop.lift` — imports Rust locais, structs externas, parâmetros/retornos Option e `console.log`.
- `examples/stdlib.lift` — comprimentos, pertinência, texto Unicode e trim compatíveis com FFI.
- `examples/io.lift` — argumentos, stdin e leitura/escrita de arquivos UTF-8.
- `examples/registry.lift` — import de `num-integer` 0.1.46 com versão exata.
- `examples/modules/main.lift` — módulos recursivos, declarações globais, imports repetidos e ciclos.
- `examples/liveness.lift` — último uso, joins, retornos, loops e reatribuição validados com compilação Rust.
- `examples/methods-closures-templates.lift` — mutação do receiver, captures snapshot, templates aninhados e ordem de avaliação.
- `examples/option.lift` — valores opcionais, campos opcionais omitíveis, narrowing de fluxo, fallbacks encadeados preguiçosos e efeitos exatamente uma vez.
- `examples/errors.lift` — construtores de Result, propagação com `?`, `throw` e `try`/`catch` tipado.
- `examples/higher-order.lift` — tipos de função, callbacks armazenados/retornados, captures e callbacks FFI.
- `examples/loops.lift` — `for...of`, `while`, `break`/`continue`, escopo de laço e iteração de arrays.
- `examples/array-methods.lift` — `map`, `filter`, `find`, `some`, `every`, `forEach`, `reduce`, encadeamento e callbacks.
- `examples/match.lift` — `match` exaustivo para `Option`, `Result`, sucesso `void` e controle de laço nos braços.
- `examples/enums.lift` — enums nominais com payloads posicionais, match exaustivo de variantes, igualdade e arrays de enums.
- `examples/notes-service/main.lift` — inicia o serviço local HTTP/SQLite da PoC.
- `examples/notes-service/db-smoke.lift` — exercita `save_note` e `list_notes` pela FFI segura.

Execute a suíte do workspace com:

```sh
cargo test --locked --workspace
cargo fmt --all --check
```

`lift_db`, `lift_web` e o projeto de fuzzing têm workspaces próprios; seus checks
de formatação e integração também fazem parte do CI. A [matriz de testes e aceite](docs/testing-and-release.pt-BR.md)
relaciona os contratos a cada camada e descreve os gates de release, fuzzing e VS Code.

Os testes da CLI copiam o executável para outro diretório e executam `check`, `build` e `run` com o core embutido. Eles inspecionam metadados Cargo, exercitam argumentos/stdin/arquivos e verificam falhas de runtime. O teste registry fica fora da suíte padrão porque pode acessar a rede; execute-o explicitamente para o aceite de release:

```sh
cargo test --locked -p lift_cli --test cli_end_to_end registry_import_consumes_a_real_pinned_rust_crate -- --ignored --exact
```

Esse teste verifica, compila e executa o exemplo registry com versão fixada. O workflow [Lift release acceptance](.github/workflows/release-acceptance.yml) executa-o em tags `v*` ou por despacho manual, além da suíte workspace, MSRV, formatação e aceites PostgreSQL/HTTP/SQLite. Os testes executados cobrem os contratos listados; não demonstram compatibilidade com toda crate Rust segura ou toda plataforma.

## Mapa do repositório

Consulte o [mapa de arquivos-fonte](docs/source-map.pt-BR.md) para ver o papel de cada crate, módulo, exemplo e documento de contrato. Para o guia em inglês, veja [README.md](README.md) e [docs/source-map.md](docs/source-map.md).
