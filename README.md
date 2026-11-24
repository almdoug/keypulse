# KeyPulse

Envia combinações de teclas (ex.: `Ctrl+Alt+F1`, `Ctrl+F2`) e ações de mouse (cliques e scroll) para qualquer janela aberta, a cada X segundos.

## Usar

Abra `target\release\keypulse.exe` — sem instalação nem login.

1. Escolha a janela na lista à esquerda.
2. Monte a sequência de combinações (grave pelo teclado ou escolha na paleta).
3. Defina o intervalo, a pausa entre combinações e quantas repetições.
4. Clique em **Iniciar** (ou `Ctrl+Shift+F12` de qualquer lugar).

A configuração é salva automaticamente em `%APPDATA%\KeyPulse\config.json`.

**Intervalo próprio por tecla**: no editor de combinação, ligue *Intervalo próprio* para que aquela tecla repita no seu próprio ritmo (ex.: `F1` a cada 2 s e `F5` a cada 10 min). As demais seguem juntas, como sequência principal, no intervalo geral. Quando dois envios vencem ao mesmo tempo, saem juntos numa só troca de foco. Com *Limitado*, cada tecla independente e a sequência principal repetem o número de vezes definido.

**Rota de caminhada**: ligue o cartão *Rota de caminhada* para andar de box em box em loop enquanto a sequência continua rodando. A rota é uma lista de trechos: *Parado* (N segundos) e *Andar* (uma seta × N sqm). O *Tempo por sqm* (padrão 300 ms) é a espera entre uma seta e a próxima; se o personagem andar menos sqm que o esperado, aumente esse valor. Exemplo: Parado 10 s → → × 10 → Parado 10 s → ← × 10 (volta ao início). Durante a execução o trecho atual fica destacado com o progresso (ex.: `6/10`).

**Gravar a rota**: em vez de montar os trechos à mão, clique em **Gravar**, vá para o jogo e faça o caminho da cave apertando as setas (1 toque = 1 sqm), terminando onde começou. Pare com **Ctrl+Shift+F12** (ou *Parar e usar*). Só as setas apertadas com a janela alvo em primeiro plano entram na gravação; o tempo antes do primeiro toque é ignorado. Toques seguidos na mesma direção viram *Andar × N*, pausas de 1 s ou mais viram *Parado* e o tempo entre o último toque e o fim vira a parada final antes de recomeçar. A gravação substitui os trechos atuais, que podem ser ajustados depois.

**Salvar, importar e limpar a rota**: **Salvar** grava a rota (trechos e *Tempo por sqm*) num arquivo `.json`, e o nome do arquivo vira o nome da hunt. Mande esse arquivo para outro jogador, que usa **Importar** para carregar a mesma rota; a importação substitui os trechos atuais. **Limpar tudo** apaga todos os trechos de uma vez (clique duas vezes para confirmar).

**Mouse**: a paleta tem *Clique esquerdo*, *Clique direito*, *Clique do scroll*, *Scroll ↑* e *Scroll ↓* (também combináveis com Ctrl/Alt/Shift). O clique acontece onde o cursor está, se ele estiver sobre a janela alvo; senão, no centro dela. No modo *Focar janela* o cursor vai até lá só durante o clique e depois volta.

**Modos de envio**
- *Focar janela*: traz a janela para frente e simula o teclado real (SendInput). Funciona em praticamente tudo.
- *Segundo plano*: envia as teclas direto para a janela, sem roubar o foco. Simula o estado de Ctrl/Alt/Shift na thread do app e uma ativação "de mentira", então funciona no Chrome/Edge (ex.: `Ctrl+J`) e na maioria dos apps. Jogos que leem o teclado em baixo nível (Raw Input/DirectInput) precisam do modo Focar janela.

**Janelas minimizadas** (opção *Manter minimizada*, ligada por padrão): a janela é restaurada de forma invisível (100% transparente, sem ativação, sem animação e sem receber cliques), recebe as teclas e é minimizada de novo. Funciona nos dois modos; para você ela nunca sai da barra de tarefas.

Janelas marcadas como **Admin** só recebem teclas se o KeyPulse também for executado como administrador.

## Compilar

Requer Rust (MSVC) e o WebView2 (já incluso no Windows 11).

```
cargo build --release
```

Estrutura: `src/` (Rust: listagem de janelas, envio de teclas, agendador, atalho global) e `ui/` (interface HTML/CSS/JS embutida no executável via Tauri 2).
