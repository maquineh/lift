---
name: lift-compiler
description: Desenvolver, revisar e evoluir o compilador Lift (.lift → Rust → binário), preservando seus contratos entre crates, HIR tipado e roadmap incremental. Use ao trabalhar em arquitetura, parser, semântica, codegen, stdlib ou CLI do Lift.
---

# Skill do compilador Lift

Use esta skill somente em tarefas do compilador Lift. Leia [references/architecture.md](references/architecture.md) antes de propor ou alterar contratos do compilador. Ao continuar de uma etapa anterior, consulte os documentos `references/phaseN-decisions.md` correspondentes; eles registram escolhas da implementação, não requisitos do blueprint.

O prompt modular chama a criação de codegen/CLI de “Etapa 4”; isso é distinto da “Fase 4” do roadmap, que trata de Optionals e erros. Para a etapa modular, consulte também `references/codegen-cli-decisions.md`.

## Como trabalhar

- Inspecione o estado atual do projeto e as instruções locais antes de editar. O blueprint da referência descreve o alvo; não presuma que algo já está implementado.
- Faça apenas a fase, crate ou correção pedida. Preserve dependências e contratos entre crates; não antecipe funcionalidades de fases futuras como se já fossem parte do MVP.
- Mantenha `lift_ast` sem lógica de parsing/semântica e mantenha as decisões semânticas no `lift_sema`. O codegen consome HIR validado e a CLI orquestra a geração e o Cargo.
- Preserve os invariantes explícitos do HIR na referência. Se uma decisão necessária não estiver especificada, registre a lacuna e escolha a menor decisão local reversível; pergunte ao usuário apenas se a lacuna impedir a tarefa ou mudar comportamento público.
- Separe requisitos do blueprint, decisões de implementação registradas e recursos ainda não suportados. Revise contraexemplos da fase atual sem exigir que fases futuras já estejam prontas.
- Não trate sugestões de tradução como garantias de correção. Código Rust emitido precisa respeitar os tipos e modos de passagem definidos pelo HIR.
- Use o critério de aceite da fase correspondente como referência para a tarefa. Execute builds, testes ou comandos de verificação somente quando isso fizer parte do pedido do usuário.

## Papéis de colaboração

Para mudanças amplas, contratos novos ou revisão de ownership, use colaboração especializada quando disponível e útil; tarefas pequenas podem ser feitas diretamente.

- **Luna (`gpt-6-luna`)**: levantamento factual e leitura delimitada, sem editar arquivos.
- **Sol (`gpt-6.1-sol`)**: coordenação, consolidação das evidências e escrita. Mantenha um único escritor para evitar alterações concorrentes.
- **Astra (`gpt-6-astra`)**: revisão crítica independente de contratos, casos-limite e conclusões; não edita a implementação.

Quando a mudança justificar o fluxo completo, siga Luna → Sol → Astra: levantamento, consolidação/implementação e revisão crítica. Passe a cada agente um escopo e artefatos concretos. Separe fatos do código, requisitos do blueprint, inferências e lacunas. Só registre o perfil/modelo que a ferramenta realmente expuser; não alegue identidade efetiva quando ela não estiver disponível.

## Fluxo de fase

Antes de implementar, identifique a fase solicitada e os crates que ela pode alterar. Para mudanças que atravessem crates, descreva o contrato de entrada/saída afetado e mantenha o escopo coerente com o roadmap. Ao concluir, informe arquivos alterados, decisões tomadas, lacunas restantes e verificações efetivamente executadas.
