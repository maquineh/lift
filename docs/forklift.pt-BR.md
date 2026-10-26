# Forklift: pacotes Truxt locais

[English](forklift.md) · [Guia de uso](../README.pt-BR.md) · [API/LSP](project-analysis-and-lsp.pt-BR.md)

Forklift resolve bibliotecas `.truxt` locais por path. `truxt.toml` ativa namespaces
de pacotes e exports explícitos de funções. Programas sem manifesto preservam
`import module` flat. Este marco não inclui registry Truxt, seleção de versões,
dependências remotas, reexports ou lockfile Forklift. Banco e HTTP/callbacks
continuam trabalhos futuros separados.

## Manifesto e imports

Aplicação `app/truxt.toml`:

```toml
[package]
name = "app"
version = "0.1.0"
kind = "bin"
entry = "main.truxt"

[dependencies]
math = { path = "../math" }
```

Biblioteca `math/truxt.toml`:

```toml
[package]
name = "math"
version = "0.1.0"
kind = "lib"
entry = "src/lib.truxt"
```

`name`, `version`, `kind` e `entry` são strings obrigatórias; nome/versão precisam
ser metadados não vazios. Versão não é constraint semver nem identidade do pacote.
Campos desconhecidos e dependências que não sejam path são rejeitados. Caminhos
usam o manifesto declarante como base; entry exige `.truxt` dentro da raiz do
pacote. Path da dependência indica diretório contendo `truxt.toml`; dependências
exigem `kind = "lib"`. Dependências transitivas são carregadas; ciclos entre
pacotes são rejeitados. O diagnóstico identifica a aresta e o destino repetido,
sem mostrar ainda toda a cadeia de dependências.

A identidade é o caminho canônico do manifesto. Dois aliases do mesmo manifesto
deduplicam; diretórios diferentes com nome/versão iguais são pacotes distintos.
Package IDs pertencem a um grafo resolvido, não a um registry global. Chaves de
dependências precisam ser identificadores Truxt; `console`, `option` e `result`
são reservados como aliases de dependência/namespace para preservar builtins.

Em `math/src/lib.truxt`:

```truxt
function helper(value: int): int { return value + 1; }
export function add(left: int, right: int): int {
    return helper(left) + right;
}
```

Em `app/main.truxt`:

```truxt
import * as math from "math";
console.log(math.add(2, 3));
```

O texto entre aspas indica chave declarada de dependência, não caminho de arquivo
ou crate Rust. O alias local pode mudar: `import * as calc from "math"; calc.add(2,
3);`. Imports estabelecem namespace de pacote, compartilhado pelos módulos flat;
repetir mesmo alias/destino é permitido, destinos conflitantes são erros. Aliases
não podem conflitar com declarações de função/interface/FFI do pacote. Variáveis
e parâmetros locais podem sombreá-los: chamadas de membro passam a usar esse
valor e seus métodos nominais. Namespace não é valor; extração de função,
named/default imports e imports dinâmicos não são suportados. `export` e `as`
continuam identificadores contextuais fora da sintaxe suportada.

Somente `export function` atravessa a fronteira do pacote. Funções privadas e
interfaces nominais homônimas em pacotes distintos coexistem. Assinaturas públicas
aceitam primitivos e containers válidos de arrays, Option e `Result<T, E>`,
recursivamente; `void` segue as regras de posição existentes. Interfaces customizadas
e closures são proibidas em qualquer ponto da assinatura pública. Não há exports
de interfaces/tipos, constantes públicas, métodos ou reexports. Interfaces/métodos
privados e closures locais continuam disponíveis dentro da biblioteca. Empréstimos,
propagação de mutação e clonagem preservam as regras Truxt entre pacotes.

`import module "relative.truxt";` continua flat dentro do pacote, permitindo
repetições e ciclos. Em projetos manifestados não pode sair da raiz nem atravessar
outro manifesto; use dependência com namespace. Somente a entry declarada de pacote
bin executa statements de topo. Bibliotecas e módulos importados contêm somente
declarações. Sem manifesto, módulos legados preservam o contrato anterior de
caminhos relativos sem essa fronteira de pacote.

## Comandos e saída do build

Compile ambos executáveis com `cargo build -p truxt_cli --locked`. `cargo run -p
truxt_cli -- ...` continua usando `truxt` por padrão; `--bin forklift` seleciona
Forklift.

```sh
forklift init my-app
forklift init --lib my-library
forklift resolve examples/packages/app
forklift check examples/packages/app
forklift build examples/packages/app
forklift run examples/packages/app -- --flag "a b"
```

Argumentos de projeto usam `.` por padrão e aceitam diretório, `truxt.toml` ou fonte
para localizar o manifesto ancestral mais próximo. A entry declarada no manifesto
é autoritativa. `init` cria `truxt.toml` e `main.truxt`; deriva nome ASCII seguro do
diretório, com fallback para nomes inteiramente não ASCII. Prepara os dois arquivos
antes da publicação, recusa sobrescrita e limpa os arquivos novos publicados caso
a publicação falhe. Usa hard links locais; os dois arquivos não constituem uma
transação indivisível.

`resolve` valida manifestos, paths locais, kinds/ciclos, carga de fontes e parsing,
sem tipos nem Cargo. `check` também verifica visibilidade/tipos/ownership e executa
`cargo check`. `build` e `run` exigem root bin e compilam em release. `check` aceita
bibliotecas. Falhas de fonte/Cargo retornam código não zero. O grafo é resolvido
uma vez por comando; a semântica consome a AST/textos capturados sem reler arquivos.

O projeto gerado continua em `.truxt_cache/<nome-da-entry>/`, ao lado da entry
declarada. `build` copia `<nome-da-entry>` (ou `.exe`) para junto dela e substitui
binário existente no mesmo destino, preservando o comportamento Truxt anterior.
`run` executa o binário do cache, encaminha argumentos e herda cwd/stdin/stdout/
stderr. O script de empacotamento inclui ambos executáveis, exemplos e manifestos.
Cargo/Rust/linker continuam requisitos externos.

## Pacotes versus crates Rust

Forklift combina pacotes Truxt locais em um HIR validado e um programa Rust/Cargo.
A semântica resolve namespaces antes de registrar identidades qualificadas no HIR;
identificadores fonte não são renomeados textualmente. Binding IDs são rebaseados
entre pacotes, preservando contratos de liveness e capturas. Codegen consome essas
identidades e os modos de ownership existentes.

FFI Rust continua `import function ... from "crate@version::path"`. São dependências
de crates Rust, resolvidas por Cargo no manifesto gerado, distintas dos pacotes
`.truxt` Forklift. Cargo pode baixar crates de registry Rust. Versões conflitantes
de crates entre pacotes Truxt são rejeitadas antes do codegen. Cargo pode criar seu
lockfile usual no cache; Forklift não possui lockfile ou resolvedor semver próprio.

## API e escopo do editor

`ProjectSession::resolve(input)` retorna metadados, `ProjectAst` contextual,
SourceMap e diagnósticos. `Resolution::analyze(self)` consome o snapshot usando
sempre a entry declarada; input apenas localiza o manifesto.
`truxt_sema::analyze_project(ProjectAst)` não acessa filesystem. A API legada
`analyze(ProgramAst)` mantém namespace flat único e rejeita sintaxe de pacote não
resolvida. Novas variantes AST exigem adaptação de matches exaustivos Rust; as
assinaturas existentes permanecem preservadas.

`ProjectSession::analyze(fonte)` e `truxt check/build/run/lint fonte` descobrem o
manifesto ancestral. A fonte solicitada precisa pertencer ao grafo declarado;
caso contrário `package.outside-graph` aponta a ela, evitando sucesso silencioso
para arquivo não importado. Sem manifesto, a fonte é entrada legada.

LSP usa as mesmas regras e overlays `.truxt` não salvos. Com `truxt.entry` explícita,
diagnósticos cobrem somente seu grafo; outros arquivos estão fora do escopo
selecionado. Com entry vazia, cada fonte aberta seleciona seu projeto ancestral ou
entrada legada, incluindo diagnóstico explícito fora do grafo quando aplicável.
Mudanças salvas em `truxt.toml` disparam reanálise pelo watcher VS Code. Buffers
TOML não salvos não são sincronizados pelo cliente inicial. Não se afirma smoke
test visual no host da extensão; testes automatizados cobrem API, protocolo e
execução do programa gerado.
