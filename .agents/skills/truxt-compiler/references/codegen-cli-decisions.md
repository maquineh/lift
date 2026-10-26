# Decisões da Etapa modular 4: codegen e CLI

Este documento registra codegen/CLI e empacotamento. HIR validado continua sendo a fronteira semântica. Result limitado e Option/?? v1 estão em phase4-decisions.md e docs/option*. Unions e propagação automática de erros continuam pendentes; Result ainda não tem construtores.

## Codegen

- emit_rust(&HirProgram) -> String gera um crate Rust executável. Structs derivam Clone, Debug e PartialEq; funções, tipos e campos Truxt são emitidos com prefixos internos e nomes ASCII codificados dos bytes UTF-8 para evitar palavras reservadas e colisões de normalização Unicode do Rust.
- Statements de topo são emitidos em fn main(). Uma função Truxt chamada main permanece função auxiliar; para executá-la, o programa a chama explicitamente nos statements de topo.
- const e let Truxt viram let Rust; requires_mut_binding decide se o binding Rust recebe mut. Isso preserva a semântica Truxt de const (sem rebinding) para além das limitações de mutabilidade de projeção do Rust.
- Os tipos são i64, f64, String, bool, (), structs prefixadas e Vec<T>. Literais de string são materializados como String; parâmetros usam ParameterPassMode; BorrowImm de string vira &str; reborrow de parâmetros já emprestados é explícito (&*p / &mut *p).
- Clones seguem exatamente CloneMode: Value usa .clone(), BorrowedAggregate clona o agregado atrás da referência e BorrowedString usa .to_owned(). ByClone materializa um owned value uma única vez.
- HirExprKind::Move registra a otimização limitada de último uso validada pela semântica e emite somente seu Local owned. Codegen não consulta CFG/liveness nem relaxa ByClone; veja phase3-decisions.md.
- pre_evaluation_temp é baixado como temporário local dentro de uma block expression. Os temporários são avaliados na ordem dos IDs locais à chamada; ByRef usa referência ao snapshot materializado. defer_borrow_until_call só cria o empréstimo ao compor a chamada, depois dos argumentos materializados. O codegen não estende esses casos além dos padrões que truxt_sema aceita.
- Expressões de string + viram format!("{}{}", ...). Literais são construídos como String owned e decodificam barra invertida, aspas, apóstrofo, n, r, t e zero; escapes desconhecidos são tratados como escapes de identidade (a barra não integra o valor). Templates primitivos usam conversão imediata e concatenação ordenada conforme methods-closures-templates-decisions.md. Escapes Unicode continuam fora.
- Em == e !=, parâmetros aggregate emprestados são comparados através do valor por dereference de place; string emprestada é convertida a String owned para igualar a representação do HIR. Isso pode clonar strings durante a comparação, preservando tipo e evitando mover parâmetros.
- Índices Truxt (int/i64) são convertidos por usize::try_from(...).expect(...), evitando wrap silencioso de índices negativos. Índice negativo ou fora do intervalo de usize termina em panic de runtime, assim como acesso fora dos limites do Vec.
- Arrays vazios usam Vec::<T>::new() com o elemento resolvido no HIR. Arrays não vazios usam vec![...].
- ExprDiscardMode::BorrowPlace emite descarte de referência (let _ = &(place);) para não mover um place não-Copy.

## Projeto Cargo e CLI

- O executável chama-se truxt, conforme o exemplo de uso do blueprint; truxtc não é alias nesta etapa.
- build <arquivo.truxt> gera src/main.rs e Cargo.toml em .truxt_cache/<nome-do-arquivo>/, chama cargo build --release com target-dir fixo no cache e localiza o executável pela mensagem compiler-artifact do formato JSON do Cargo. Depois copia o executável ao lado da fonte (.exe no Windows).
- run <arquivo.truxt> [-- argumentos...] gera/compila em release e executa o binário, encaminhando argumentos e herdando o diretório atual, stdin, stdout e stderr do usuário. Cargo respeita o target configurado; se a configuração produzir binário de outra plataforma, não será executável no host.
- check <arquivo.truxt> gera o projeto e executa cargo check, verificando também o Rust gerado. A CLI usa Ariadne 0.6.0 em modo IndexType::Byte para erros de fonte, com trecho e labels principal/secundário no mesmo relatório. EOF vazio e offsets UTF-8 são preservados. Cores dependem de stderr interativo e ausência de NO_COLOR; falhas mantêm código 1. Cargo/rustc seguem com seus diagnósticos próprios.
- init [diretório] cria main.truxt com um comentário inicial e não sobrescreve um arquivo existente. O programa é script de topo; não existe manifesto Truxt nesta etapa.
- Cada fonte tem um subdiretório de cache com nome do arquivo-base. main.rs e Cargo.toml só são gravados quando os bytes mudam; o core embutido usa a mesma regra. Análise/codegen e Cargo continuam executando a cada comando. Isso é otimização de reescrita, não cache completo com chave por conteúdo/configuração; Cargo mantém o target incremental.
- O manifesto declara workspace vazio para não ser absorvido por um workspace ancestral. Imports registry usam versão exata. A CLI incorpora a stdlib e a extrai em `.truxt_cache/<fonte>/truxt_core/`, com manifesto sem herança do workspace. O programa usa `path = "truxt_core"` quando precisa do core, sem caminho do checkout em runtime. Arquivos idênticos não são regravados; mudanças na cópia extraída são sobrescritas. Rust/Cargo e linker continuam externos. Veja `phase5-decisions.md`.
- cargo é localizado pelo PATH. Falhas do Cargo retornam seu código de saída; build substitui o binário anterior com o mesmo nome-base ao lado do fonte.

## Fora do escopo

O incremento posterior de tooling move loader/SourceMap para `truxt_project`,
adiciona `truxt lint`, `truxt lsp` e extensão VS Code inicial. Build/check/run
mantêm o contrato de emissão/Cargo acima. Veja `tooling-decisions.md` e
`docs/project-analysis-and-lsp.md` / `docs/project-analysis-and-lsp.pt-BR.md`.

- Unions, try/catch, callbacks/escape de closures, FFI unsafe, generics gerais e structs FFI continuam fora. Métodos nominais, closures locais snapshot e templates primitivos seguem methods-closures-templates-decisions.md. Option/??, Result especial, console.log, imports seguros e Ariadne têm contratos registrados. Option fica totalmente fora do FFI, inclusive retornos.
- O codegen consome HIR produzido por analyze; wrappers de import gerados deixam o rustc verificar caminho e assinatura Rust. Os testes de integração da CLI validam `check`, `build` e `run` com `examples/fibonacci.truxt`, `examples/ownership.truxt` e `examples/interop.truxt`; o E2E interop também confere stdout. Isso cobre os caminhos e recursos usados pelos exemplos, sem provar todas as formas possíveis de HIR ou imports.
- Imports explícitos `import module "relative.truxt";` carregam múltiplos arquivos em uma tabela global, sem manifesto/namespaces. Somente a entrada aceita statements de topo. O grafo é relido em cada comando; não há chave completa de cache por conteúdo. Veja `modules-decisions.md`.

## Aceite de distribuição v0.1

- O E2E copia e move a CLI para diretório temporário, executa check/build/run de `stdlib.truxt` e confirma por cargo metadata que o core vem do cache.
- `io.truxt` cobre Result, argumentos, stdin e arquivos UTF-8. O teste registry optativo compila/executa `num-integer` 0.1.46 e fica fora da suíte padrão por usar rede.
- `scripts/package.ps1` compila para o host Rust atual e cria ZIP com executável, guias bilíngues e exemplos. O pacote não instala Rust/linker e não altera PATH.

## Incremento posterior: diagnósticos e reescrita

- Ariadne é dependência apenas da CLI (versão exata 0.6.0 centralizada no workspace); AST, lexer, parser e sema mantêm seus spans de bytes e contratos.
- O teste de arquivos gerados fixa mtimes históricos e verifica preservação sem sleeps; mudanças no Rust e manifesto são verificadas separadamente.
- O incremento multi-arquivo posterior definiu imports relativos canônicos e spans com SourceId, preservando o renderer Byte e a otimização de reescrita. Manifesto/namespaces/cache completo continuam fora do contrato. Veja `modules-decisions.md` e guias bilíngues em docs/modules.
