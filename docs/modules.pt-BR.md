# Módulos explícitos de arquivos

[English](modules.md) · [Guia de uso](../README.pt-BR.md)

Este documento descreve imports flat legados, preservados sem manifesto.
O [incremento Forklift](forklift.pt-BR.md) acrescenta `lift.toml`, namespaces
de pacotes e funções exportadas; módulos continuam flat dentro de cada pacote.

`import module "relative.lift";` é declaração de topo, distinta dos imports Rust FFI. O ponto e vírgula é obrigatório. `module` é contextual nessa posição e continua disponível como identificador comum. Imports aninhados, aliases, namespaces, exports, busca de pacotes e manifesto ficam fora deste incremento.

Cada caminho é relativo ao arquivo importador, independentemente do diretório de trabalho do chamador. Exige extensão `.lift`, sem distinção de maiúsculas como na entrada da CLI. Caminhos absolutos/enraizados ou com prefixo de drive são rejeitados. `..` é permitido. O especificador usa o texto literal do token, sem interpolação ou decodificação de escapes de strings de runtime; use `/` para portabilidade.

A CLI canonicaliza e deduplica arquivos antes de seguir seus imports. Cada fonte canônica é lida/parseada uma vez na compilação. A marcação ocorre antes da recursão, permitindo repetições, ciclos entre módulos e retorno ao arquivo de entrada sem duplicar declarações ou executar statements root novamente. Imports não têm etapa de execução em runtime.

## Declarações e execução

Somente a entrada/root pode ter statements executáveis de topo: `const`/`let`, atribuições, chamadas, condicionais e loops. Statements root continuam sujeitos à semântica anterior; `return` exige corpo de função. Módulos importados podem declarar funções, interfaces, imports Rust seguros e outros módulos. Um statement no módulo é erro mesmo que não produza saída. Módulos vazios são válidos.

Funções, interfaces e declarações FFI entram na tabela global existente. Chamadas adiantadas, chamadas entre arquivos e tipos compartilhados funcionam após carregar todo o grafo. Duplicatas globais preservam spans principal/secundário, permitindo labels nos dois arquivos. Não há visibilidade privada por arquivo. Regras anteriores continuam: funções não capturam variáveis de topo, e nomes duplicados não simulam namespaces.

O loader remove imports de módulo antes da análise semântica. A função pública `analyze` não acessa filesystem; imports de módulo não resolvidos passados diretamente a ela são rejeitados sem HIR parcial.

## Identidade de fonte e diagnósticos

`Span` agora contém `SourceId` e offsets de bytes UTF-8 semiabertos. `Span::new` e `tokenize` mantêm ID zero para consumidores anteriores de fonte única. `Span::in_source`, `tokenize_with_source` e `parse_with_source` permitem IDs explícitos; `parse` preserva o ID dos tokens, que devem ser de uma única fonte. `parse_with_source` normaliza todos os spans para o ID explícito, inclusive tokens da API legada, e dá identidade a um programa vazio. Join de fontes diferentes é rejeitado.

Adicionar o campo público `source` quebra o código de consumidores externos que construíam `Span { start, end }` diretamente; eles devem informar `source` ou usar construtor. Preservar construtores e entradas de fonte única não significa compatibilidade total de código-fonte.

A CLI mantém tabela de caminhos canônicos e textos UTF-8. Spans AST/HIR preservam IDs; Ariadne resolve cada label em modo Byte. A identidade do cache inclui SourceId independentemente do caminho exibido, evitando colisões na conversão de caminhos OS para Unicode com perda. Erros léxicos, sintáticos, semânticos e EOF em módulo apontam para ele. Arquivos ausentes/ilegíveis/sem UTF-8 válido são marcados no literal do importador. Falhas de fonte mantêm código `1`.

## Geração e limites

O grafo produz um programa tipado, uma fonte Rust e um executável no cache da entrada. Reuso de bytes gerados e incremental do Cargo continuam. O grafo inteiro é carregado/analisado em cada comando; não é cache de hashes de dependências. Uma mudança transitiva afeta o próximo comando mesmo com entrada inalterada.

`examples/modules/main.lift` exercita interfaces/funções compartilhadas, FFI em módulo, imports relativos/transitivos, caminhos repetidos, ciclos entre módulos e ciclo pela entrada. Testes conferem stdout e execução única do root, mudança transitiva, statements proibidos, erros UTF-8/EOF importados, labels cruzados, caminhos inválidos e módulos vazios. Não há suporte a carregamento dinâmico ou compilação separada.
