# Decisões registradas na Etapa 2

Este arquivo registra escolhas da implementação atual do parser; elas não estavam todas fechadas no blueprint.

- O parser recebe apenas `&[(Token, Span)]`, retorna `ProgramAst` ou uma lista de `ParseError`, e não acessa a fonte original.
- Declarações e statements usam recursive descent; expressões usam Pratt parsing.
- Binding powers: postfix `.`, `()`, `[]`; prefix unary; `*`, `/`, `%`; `+`, `-`; comparações; igualdade; `&&`; `||`. Operadores binários associam à esquerda. Os níveis adicionais mantêm coerência com tokens e nós de operador existentes e com condições de loop, como `i < n`.
- Campos de interface aceitam vírgulas opcionais, como na EBNF. Object literals, arrays, parâmetros e argumentos usam vírgulas entre itens e aceitam vírgula final.
- Semicolons são opcionais ao fim de variable declarations, assignments, returns e expression statements, pois a EBNF não os exige e o lexer descarta newlines. Não há inserção de semicolon baseada em newline.
- `return` consome um valor quando o próximo token pode iniciar expressão; caso contrário é return sem valor e o token atual inicia a próxima construção.
- O parser também aceita imports FFI de função com assinatura explícita: `import function nome(args): tipo from "pacote@versão::caminho::função";`. A forma completa, tipos, validação e limites estão em `phase5-decisions.md`; o `;` do import é obrigatório.
- `T[]` é reconhecido em tipos; `T?`, `??`, arrow functions, template strings e `impl` ficam fora desta etapa.
- A atribuição é reconhecida no nível de statement depois de uma expressão. A AST preserva o alvo; validar se ele é atribuível pertence à semântica.
- O parser acumula erros com spans e sincroniza em semicolon, fim de bloco ou início de declaração/statement, considerando delimitadores aninhados. Se a lista contiver qualquer erro, `parse` retorna os erros, não uma AST parcial. Spans de EOF usam o fim do último token, pois o contrato não recebe a fonte nem os espaços finais.
