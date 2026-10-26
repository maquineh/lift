# Decisões registradas na Etapa 1

Este arquivo registra escolhas da implementação atual para orientar as próximas etapas. Elas não estavam todas definidas no blueprint; altere-as somente de forma explícita e coordenada com os consumidores.

- O workspace atualmente contém `crates/lift_ast`, `crates/lift_lexer`, `crates/lift_parser`, `crates/lift_sema`, `crates/lift_codegen`, `crates/lift_cli` e `stdlib/lift_core`. A implementação do parser, da semântica, da etapa modular de codegen/CLI e do MVP de interop está delimitada em `phase2-decisions.md`, `phase3-decisions.md`, `codegen-cli-decisions.md` e `phase5-decisions.md`. A stdlib oferece API Rust-native sem dependências externas; imports explícitos e `console.log` em `.lift` são suportados somente dentro dos limites registrados na decisão da Fase 5.
- No `lift_core`, arrays são slices emprestados; `map/filter/find` recebem closures por `&mut F: FnMut(&T)`, com `map -> Vec<U>` (podendo emprestar do slice de entrada), `filter -> Vec<&T>` e `find -> Option<&T>`, sem clone implícito dos elementos. `string::len` conta valores escalares Unicode (`char`), `concat` retorna `String` e `trim` retorna `&str` emprestado. `Console` recebe `Display`, acrescenta `\n` e propaga erros com `std::io::Result<()>`. Essas são decisões locais da API Rust, não contratos de integração com a linguagem.
- `Span` é um intervalo zero-based e semiaberto de offsets em bytes UTF-8. `lift_ast::Span` é compartilhado por toda a AST e pode ser convertido para/de `Range<usize>`.
- Identificadores aceitam Unicode XID e `_`. Palavras-chave e tipos primitivos são tokens próprios.
- `IntegerLiteral` carrega magnitude `u64`; `-` continua token separado. A fase semântica deve validar o intervalo de `i64` considerando o contexto unário, inclusive `i64::MIN`.
- `NumberLiteral` carrega `f64` finito; valores que resultam em infinito, como `1e999`, são erro lexical.
- Strings com aspas simples ou duplas guardam o conteúdo sem delimitadores e preservam escapes sem decodificá-los. Literais não podem atravessar linha. Template strings e interpolação ficam para uma fase posterior.
- Espaços, comentários de linha e comentários de bloco fechados são ignorados. Strings e comentários de bloco sem fechamento produzem erro com span.
- O lexer reconhece o operador futuro `=>` sem implementar closures. `??`, o sufixo de tipo `?` e o literal reservado `null` já têm parsing/semântica no contrato Option v1 da Fase 4. `undefined` não é valor de ausência.
- `tokenize` para na primeira falha e retorna `LexError` com categoria e span. Consumidores devem usar esse contrato, sem depender do tipo de erro interno exigido pelo derive do Logos.
