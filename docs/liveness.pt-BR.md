# Moves conservadores de último uso

[English](liveness.md) · [Guia de uso](../README.pt-BR.md)

Após a análise semântica aprovar o programa, uma passagem privada monta CFG por função e para o script de entrada. Ela não altera sintaxe, conjunto aceito, modos de parâmetros, diagnósticos de aliasing ou a semântica de cópia de valores do Lift. Pode remover uma cópia redundante quando o valor owned original não será mais necessário.

Cada statement simples e cada condição `if`/`while` possui um ponto. Ramos convergem na continuação real; `return` não tem sucessor. Loops incluem backedge para a condição e saída de zero iterações. Liveness reversa calcula `live_out = união(live_in(sucessores))` e `live_in = usos ∪ (live_out − defs)` até ponto fixo. Rebinding direto mata o valor anterior; mutação de campo/índice não mata sua raiz. Chamadas usam conservadoramente todos os argumentos.

Somente `Clone(Value, Local)` de local owned direto pode virar `HirExprKind::Move` explícito. O binding deve ocorrer uma vez no statement completo e estar ausente do `live_out` do ponto em todos os caminhos. Contadores de condições incluem o if/while completo, com ambos os ramos/corpo; statements filhos possuem pontos e contagens próprias. O codegen só emite o binding decidido, sem inferir moves novamente.

Owned inclui declarações locais e parâmetros `ByValue`/`OwnedMove`. Parâmetros `BorrowImm`/`BorrowMut` são excluídos. A passagem preserva argumentos `ByClone`, clones de string/agregado emprestado, campos/índices, operandos agrupados, ocorrências repetidas no statement e casos incertos. Não otimiza statements inalcançáveis, não dobra condições constantes e não faz análise geral de aliasing interprocedural. Retornos de parâmetros owned podem já ser Local no HIR validado; esta passagem só troca clones explícitos elegíveis.

A prova tem granularidade de statement. Não prova último uso geral dentro de qualquer ordem de avaliação nem elimina todas as cópias possíveis. Nenhum padrão de ownership antes rejeitado passa a ser aceito. Consumidores externos que fazem match exaustivo no HIR também precisam tratar a nova variante Move.

O incremento Option observa payloads e ambos os operandos de coalesce como usos possíveis, mantendo clones internos a Some/Coalesce. Isso preserva liveness conservadora ao redor do fallback condicional; consulte [Option](option.pt-BR.md).

Testes cobrem cópias elegíveis em funções/script, usos posteriores, joins if/else/else-if, returns terminais, ponto fixo/zero iterações, reatribuição, repetições, parâmetros emprestados, projeções e `ByClone` preservado. A CLI passa `examples/liveness.lift` por check/build/run e confere stdout do executável nativo. A evidência cobre os casos exercitados, sem alegar prova geral de Borrow Checker.
