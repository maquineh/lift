# Truxt

Rust/Cargo **1.94.1+** (MSRV: `1.94.1`).

Truxt é uma linguagem pequena e estaticamente tipada que compila uma entrada `.truxt` e seus módulos explícitos para Rust, usando Cargo para verificar, compilar ou executar.

> **Escopo atual:** programas tipados com módulos relativos explícitos, biblioteca padrão embutida, imports Rust seguros, console e I/O UTF-8 baseado em Result. O executável pode ser movido independentemente do checkout. Rust, Cargo e um linker do host continuam necessários para compilar programas.

Truxt busca ser familiar para quem usa TypeScript dentro de um subconjunto documentado; não é uma implementação completa de TypeScript. Cada recurso novo deve definir a semântica Truxt e gerar Rust válido, considerando ownership, borrowing e segurança do Rust.

## Requisitos

- Rust **1.94.1+**, com `rustc` e `cargo` disponíveis no `PATH`.
- Linker funcional para o target hospedeiro do Rust.
- Acesso à rede quando o programa importar uma crate registry que ainda não esteja no cache do Cargo.

O ambiente validado atualmente é Windows x86_64 com Rust/Cargo 1.94.1 e target hospedeiro MSVC. A versão mínima suportada (MSRV) é 1.94.1; outros hosts ainda não foram validados. Compile e execute para o host; binários de compilação cruzada podem não executar nesta máquina.

## Compile a CLI a partir deste checkout

Na raiz do repositório:

```sh
cargo build --release -p truxt_cli
```

O executável será gravado em `target/release/truxt` (ou `truxt.exe` no Windows). As fontes da stdlib são incorporadas durante sua compilação; o executável não precisa mais do checkout original.

## Instalação ou pacote portátil

Instale a partir do checkout no diretório de executáveis do Cargo:

```sh
cargo install --path crates/truxt_cli --locked
truxt --version
```

Também é possível copiar o executável release para um diretório no `PATH`. Somente o executável é necessário do Truxt; Rust/Cargo e o linker do host continuam pré-requisitos. No PowerShell, crie um ZIP com executável, guias bilíngues e exemplos:

```powershell
./scripts/package.ps1
```

Extraia `dist/truxt-<versão>-<host>.zip` em qualquer pasta e adicione essa pasta ao `PATH`, ou chame o executável pelo caminho. O script compila para o host atual do Rust. Ele não instala Rust nem altera o `PATH`.

## Início rápido

Crie um arquivo inicial e execute-o:

```sh
cargo run -p truxt_cli -- init ola
```

Edite `ola/main.truxt`:

```truxt
console.log("Olá, Truxt!");
```

Execute o programa:

```sh
cargo run -p truxt_cli -- run ola/main.truxt
```

Com o executável compilado no `PATH`, os comandos equivalentes são `truxt init ola` e `truxt run ola/main.truxt`.

## Comandos da CLI

| Comando | Comportamento |
| --- | --- |
| `truxt init [diretório]` | Cria `main.truxt`; recusa sobrescrever um arquivo existente. |
| `truxt check arquivo.truxt` | Analisa Truxt, gera Rust e executa `cargo check`. |
| `truxt build arquivo.truxt` | Compila em modo release e copia o executável para perto do arquivo fonte. |
| `truxt run arquivo.truxt [-- args...]` | Compila em modo release e executa o binário, encaminhando os argumentos. |

Os projetos e artefatos Cargo ficam em `.truxt_cache/<nome-da-entrada>/`, ao lado da entrada. A CLI extrai o core em `truxt_core/` nesse projeto e usa dependência Cargo relativa. `main.rs`, `Cargo.toml` e core só são gravados quando seus bytes mudam. Alterações no core extraído são sobrescritas no próximo comando. Isso evita reescritas desnecessárias; carregamento de módulos, análise, codegen e Cargo continuam executando a cada chamada, portanto não é cache completo por chave de conteúdo.

## Módulos de arquivos

Importe outra fonte Truxt no topo:

```truxt
import module "lib/math.truxt";
console.log(double(21));
```

`lib/math.truxt` pode declarar a função:

```truxt
function double(value: int): int { return value * 2; }
```

Caminhos são relativos ao arquivo importador e exigem `.truxt`. Imports são recursivos; cada caminho canônico entra uma vez, permitindo repetições e ciclos. Somente a entrada pode conter statements executáveis de topo. Módulos importados podem declarar funções, interfaces, imports Rust FFI e outros módulos. Declarações compartilham namespace global; duplicatas geram erro com labels nos dois arquivos. Ainda não há aliases, namespaces de módulo, exports ou manifesto de projeto. A sintaxe Rust `import function ... from ...` não mudou.

Execute `truxt run examples/modules/main.truxt` para imports relativos/transitivos, FFI, tipos compartilhados e ciclos incluindo a entrada. Consulte o [contrato de módulos](docs/modules.pt-BR.md) para identidade de fonte e limites exatos.

## Diagnósticos de fonte

Erros léxicos, sintáticos e semânticos usam relatórios Ariadne com caminho da fonte, mensagem, trecho, label principal e label relacionado quando o compilador fornece span secundário. Spans de bytes UTF-8 são mostrados na posição de caractere correta, incluindo spans EOF vazios. Erros de fonte mantêm código de saída `1`.

Cores só são ativadas quando stderr é um terminal interativo e podem ser desligadas com `NO_COLOR`; saídas redirecionadas não incluem cores ANSI. Diagnósticos Cargo/rustc continuam sendo os relatórios do Cargo sobre o Rust gerado, sem remapeamento para Truxt. Consulte [diagnósticos e arquivos gerados](docs/diagnostics-and-build.pt-BR.md) para o contrato da implementação.

## Escopo da linguagem e da interoperabilidade

A implementação atual oferece funções tipadas, valores primitivos (`int`, `number`, `boolean`, `string`), arrays, interfaces/literais de objetos, fluxo de controle e os padrões de ownership cobertos pelos exemplos e testes. A linguagem não insere ponto e vírgula a partir de quebras de linha. Declarações de import ficam no topo e exigem ponto e vírgula final.

Imports Rust declaram uma assinatura Truxt explícita e um caminho Rust:

```truxt
import function concat(left: string, right: string): string from "truxt_core::string::concat";
console.log(concat("Olá, ", "Truxt!"));
```

O FFI aceita funções Rust livres e seguras com escalares e arrays de um nível. Parâmetros `string` são passados como `&str`, e retornos string como `String` owned; parâmetros array são slices emprestados e retornos array são `Vec<T>` owned. `void` corresponde a `()`. Um import também pode retornar `Result<T, E>` com payloads suportados; parâmetros Result e retornos Result aninhados continuam fora do FFI. Imports registry usam versão exata `MAJOR.MINOR.PATCH`. O wrapper Rust deixa `rustc` verificar caminho e assinatura. FFI de structs, `unsafe` e arrays aninhados não são suportados.

`console.log` aceita uma expressão de `int`, `number`, `boolean` ou `string` e a escreve com uma quebra de linha. Atualmente, falhas ao escrever em stdout causam panic.

A crate Rust-native `truxt_core` inclui helpers de console, arrays e strings. Os adaptadores compatíveis com FFI ficam em `truxt_core::interop`:

```truxt
import function tamanho(values: int[]): int from "truxt_core::interop::array_len";
import function trim(value: string): string from "truxt_core::interop::string_trim";
import function contem(values: int[], value: int): boolean from "truxt_core::interop::array_contains_int";
```

Os adaptadores também oferecem `string_len` e `array_contains_number`, `array_contains_boolean` e `array_contains_string`. Os helpers com callbacks (`map`, `filter` e `find`) continuam exclusivos do Rust porque o subconjunto de closures locais ainda não aceita tipos de função higher-order ou FFI. Consulte o [guia da biblioteca core](stdlib/truxt_core/README.pt-BR.md) para a API completa.

## Valores opcionais

Use `T?`, `null` contextual e `option.some(value)` explícito:

```truxt
const absent: int? = null;
const present = option.some(7);
console.log(absent ?? present ?? 9);
```

`??` avalia a esquerda uma vez e o fallback somente na ausência. `T? ?? T` retorna `T`; `T? ?? T?` continua opcional. `option.is_some` testa e `option.unwrap` clona o payload presente; unwrap na ausência causa panic. `int?[]` é array de opcionais; `int[]?` é array opcional. Campos `T?` continuam obrigatórios. Não há lift implícito, ausência `undefined`, narrowing, campos omitíveis ou Option no FFI. Consulte o [contrato exato](docs/option.pt-BR.md), incluindo limites de inferência e parênteses entre operadores.

## Entrada e arquivos

`Result<T, E>` é um tipo especial disponível em anotações e assinaturas de funções Truxt. Use imports explícitos para argumentos, entrada padrão e arquivos UTF-8:

```truxt
import function arguments(): Result<string[], string> from "truxt_core::io::arguments";
import function read_stdin(): Result<string, string> from "truxt_core::io::read_stdin";
import function read_file(path: string): Result<string, string> from "truxt_core::io::read_file";
import function write_file(path: string, contents: string): Result<void, string> from "truxt_core::io::write_file";
```

`arguments()` retorna os argumentos sem o nome do executável, preservando ordem e espaços. Use `--` após a fonte para encaminhar flags: `truxt run app.truxt -- --flag "a b"`. `read_stdin()` lê até EOF, incluindo quebras de linha. `read_file()` exige UTF-8; `write_file()` cria ou trunca o arquivo sem criar diretórios pais. Caminhos relativos usam o diretório de trabalho do chamador, preservado por `truxt run`.

Falhas de I/O e UTF-8 inválido tornam-se strings de erro em vez de panic. Trate o resultado com um ramo:

```truxt
const contents = read_file("input.txt");
if result.is_ok(contents) {
    console.log(result.unwrap(contents));
} else {
    console.log(result.unwrap_err(contents));
}
```

A inspeção de Result empresta o argumento; unwrap clona o payload para preservar o Result original. Unwrap na variante errada causa panic. Generics gerais, construtores Result, pattern matching, `?` e `try/catch` ainda não foram implementados. Consulte o [contrato Result/I/O](docs/result-and-io.pt-BR.md) para o escopo exato. Exemplo que lê entrada canalizada e grava um arquivo:

```powershell
"Olá da entrada padrão" | truxt run examples/io.truxt -- output.txt exemplo
```

O arquivo resultante contém `Olá da entrada padrão|exemplo`. Ao executar interativamente, o programa continua lendo stdin até EOF.

## Limitações atuais

- Uma entrada por execução da CLI, com imports relativos explícitos; sem manifesto, namespaces de módulo ou aliases.
- Não há cache completo por chave de conteúdo. Arquivos gerados idênticos são preservados e Cargo reutiliza seus artefatos; análise/codegen continuam executando a cada chamada.
- A análise de ownership continua conservadora e rejeita alguns padrões complexos de aliasing. Uma passagem privada CFG/liveness remove somente clones comprovadamente de último uso de locais owned diretos; valores emprestados, projeções, usos repetidos no statement e argumentos `ByClone` mantêm cópias. Consulte o [contrato da otimização](docs/liveness.pt-BR.md).
- Métodos nominais/`impl`, arrow closures locais com snapshot e interpolação de primitivos estão implementados; veja [contratos e limites](docs/methods-closures-templates.pt-BR.md). Unions e `try/catch` continuam pendentes. Option fica fora do FFI e campos opcionais não podem ser omitidos.
- Result usa `is_ok` e unwrap com cópia; construtores, matching e propagação automática de erros não foram implementados.

## Exemplos e testes

- `examples/fibonacci.truxt` — funções recursivas e fluxo de controle.
- `examples/ownership.truxt` — mutação de agregados, empréstimos e valores de retorno owned.
- `examples/interop.truxt` — imports Rust locais e `console.log`.
- `examples/stdlib.truxt` — comprimentos, pertinência, texto Unicode e trim compatíveis com FFI.
- `examples/io.truxt` — argumentos, stdin e leitura/escrita de arquivos UTF-8.
- `examples/registry.truxt` — import de `num-integer` 0.1.46 com versão exata.
- `examples/modules/main.truxt` — módulos recursivos, declarações globais, imports repetidos e ciclos.
- `examples/liveness.truxt` — último uso, joins, retornos, loops e reatribuição validados com compilação Rust.
- `examples/methods-closures-templates.truxt` — mutação do receiver, captures snapshot, templates aninhados e ordem de avaliação.
- `examples/option.truxt` — opcionais, fallbacks encadeados preguiçosos e efeitos exatamente uma vez.

Execute a suíte do workspace com:

```sh
cargo test --workspace
cargo fmt --all --check
```

Os testes da CLI copiam o executável para outro diretório e executam `check`, `build` e `run` com o core embutido. Eles inspecionam metadados Cargo, exercitam argumentos/stdin/arquivos e verificam falhas de runtime. O teste registry fica fora da suíte padrão porque pode acessar a rede; execute-o explicitamente para o aceite de release:

```sh
cargo test -p truxt_cli --test cli_end_to_end registry_import_consumes_a_real_pinned_rust_crate -- --ignored --exact
```

Esse teste verifica, compila e executa o exemplo registry com versão fixada. Os testes locais validam as combinações de linguagem e host exercitadas; não demonstram compatibilidade com toda crate Rust segura ou toda plataforma.

## Mapa do repositório

Consulte o [mapa de arquivos-fonte](docs/source-map.pt-BR.md) para ver o papel de cada crate, módulo, exemplo e documento de contrato. Para o guia em inglês, veja [README.md](README.md) e [docs/source-map.md](docs/source-map.md).
