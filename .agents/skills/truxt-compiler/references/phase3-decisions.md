# Decisões registradas na Fase 3

Este documento registra a primeira implementação de `truxt_sema`. O blueprint continua sendo a referência para contratos; estes itens delimitam o comportamento implementado e não ampliam silenciosamente a linguagem.

## Tipos e checagem

- `int` resolve para `i64`, `number` para `f64`, `boolean` para `bool`, `string` para `String`, `void` para `()`; arrays resolvem para `Vec<T>` e interfaces para structs nominais.
- Não há coerção implícita entre `int` e `number`. O operador `/` preserva o tipo numérico dos operandos, portanto `int / int` é `int` nesta decisão inicial.
- `+` aceita dois operandos do mesmo tipo numérico ou duas strings. A concatenação de strings é marcada para lowering pelo codegen.
- Comparações ordenadas aceitam tipos numéricos iguais. Igualdade aceita tipos iguais não `void`.
- Object literals exigem tipo nominal esperado e precisam informar exatamente os campos da interface. O HIR preserva a ordem fonte dos valores para não reordenar efeitos; campos são identificados pelo nome.
- Arrays não vazios inferem o tipo pelo primeiro elemento e validam os demais. Arrays vazios exigem anotação/contexto.
- Valores retornados exigem anotação explícita; função sem anotação retorna `void`. O incremento Result permite `void` também como payload de sucesso.
- Tipos opcionais continuam rejeitados. O incremento limitado de `Result<T, E>` está em `phase4-decisions.md`; métodos, closures, valores de função e chamadas indiretas permanecem rejeitados.
- Structs suportam derives `Clone`, `Debug` e `PartialEq` com os tipos atuais. Um ciclo por campos de tamanho direto é rejeitado; um ciclo que passa por `Vec` é permitido.
- Strings preservam escapes até o codegen, que decodifica barra invertida, aspas, apóstrofo, n, r, t e zero; escapes desconhecidos são de identidade. Escapes Unicode e templates continuam pendentes, conforme `codegen-cli-decisions.md`.

## Escopos e mutabilidade

- Funções e interfaces são pré-declaradas, permitindo chamadas adiantadas e recursão.
- Variáveis locais usam escopos lexicais, mas uma declaração não pode sombrear qualquer binding ainda ativo. Reutilizar o nome depois que o escopo anterior termina é permitido.
- Statements de topo compartilham um escopo de script e podem usar bindings anteriores. Funções não capturam bindings de topo.
- `const` impede reatribuição do binding. Projeções de agregados (`obj.campo`, `array[i]`) ainda podem ser alteradas; o HIR marca `requires_mut_binding` para o detalhe Rust `let mut` sem permitir rebinding Truxt.
- Reatribuição direta de parâmetro é rejeitada. Alterar campo/elemento de parâmetro é distinto e participa da inferência de modo.

## Ownership desta implementação

- Parâmetros `int`, `number` e `boolean` usam `ByValue`. Parâmetros não `Copy` são classificados por leitura (`BorrowImm`), mutação (`BorrowMut`) ou escape detectado em retorno (`OwnedMove`). `OwnedMove` prevalece quando o parâmetro também sofre mutação; nesse caso o binding owned recebe `requires_mut_binding`.
- Chamadas a parâmetros `BorrowMut` propagam a necessidade de mutação até ponto fixo entre funções. A implementação usa IDs de binding no HIR e rejeita dois argumentos `ByMutRef` que apontem para a mesma raiz na mesma chamada. Chamadas mutáveis sequenciais sobre a mesma raiz continuam válidas.
- O HIR torna explícitos clones em inicializadores, atribuições, campos, arrays e retornos. Places não `Copy` são clonados conservadoramente quando passam a compor um valor owned. Place passado a parâmetro owned usa `ArgPassMode::ByClone`; temporários e tipos `Copy` usam `ByValue`.
- Em chamadas com leitura da mesma raiz que outro argumento emprestado mutavelmente, `pre_evaluation_temp` instrui o codegen a materializar o valor primeiro. `defer_borrow_until_call` posterga os empréstimos quando outra expressão avalia uma chamada mutável sobre a mesma raiz; nesse caso, os argumentos de valor são temporários avaliados em ordem fonte antes dos borrows. A materialização precisa ocorrer antes de qualquer `&mut`; clone inline em outro argumento não resolve o conflito. Places emprestados com chamadas internas e índices que leem a própria raiz mutável são diagnosticados até haver lowering de place explícito.
- `CloneMode` distingue clone de valor owned, clone através de referência e conversão de `&str` para `String`; `ArgPassMode::ByClone` carrega o mesmo metadado para o codegen.
- Places não `Copy` em expression statements usam `ExprDiscardMode::BorrowPlace` para não mover o valor somente por descartá-lo.
- O clone conservador evita depender de liveness incompleta. A análise de CFG/liveness reversa e a otimização de moves para último uso ainda não estão implementadas; o HIR não deve alegar essa otimização.
- A inferência de escape é conservadora e sintática: o parâmetro torna-se `OwnedMove` quando aparece como valor em retorno direto, object literal ou array literal. Campo/elemento derivado e argumento de chamada não contam como escape porque places nesses contextos são clonados quando viram valores owned. Isso não é uma análise geral de efeitos/alias.
- `pre_evaluation_temp` registra apenas conflitos diretos observáveis por raízes de bindings nos argumentos. Chamadas/efeitos com ordenação mais complexa e aliasing não representável por uma raiz simples devem receber diagnóstico antes de serem declarados seguros.

## Diagnósticos e limites

- `SemaError` carrega categoria, mensagem, span principal e span secundário opcional. Formatação visual com `ariadne` permanece para a Fase 6.
- Um HIR válido não contém `Unknown`/tipos de erro. Se qualquer erro for encontrado, `analyze` retorna a lista de erros, sem HIR parcial.
- A análise não prova segurança geral do Borrow Checker e não confirma que o codegen Rust compila. Isso depende do codegen e de validação explícita em etapa posterior.
- Os testes de `truxt_sema` cobrem recursão, resolução de tipos, modos de empréstimo, conflito de duas referências mutáveis à mesma raiz e diagnósticos de tipos/inteiros. A CLI compila e executa exemplos de Fibonacci e ownership; essa validação abrange esses casos, não todas as combinações de HIR.

## Revisão da proposta BorrowInferEngine

A divisão entre resumo interprocedural, liveness reversa e resolução de aliasing é uma boa arquitetura para a versão futura. O pseudocódigo/HTML fornecido não implementa essa garantia ainda: falta caminhar por todas as expressões e ramos, fechar o ponto fixo de loops, distinguir usos/defs por IDs, e produzir uma estratégia de temporários antes dos empréstimos. Mutação e escape são fatos independentes e não podem ser colapsados por precedência sem definir a semântica de cópia.

A revisão encontrou no HTML exemplos finais que ainda emitem `subir_nivel(&mut heroi, heroi.nivel)` e `renomear(&mut u, &u.nome.clone())`. Ambos tentam avaliar a leitura depois de iniciar o empréstimo mutável; a saída precisa materializar o argumento antes da chamada. O simulador é uma animação baseada em dados fixos (`CASES_DATA`), não executa o algoritmo nem valida o Rust emitido. Ele deve ser tratado como ilustração, não como teste.
