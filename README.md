# KeyPulse

Envia combinações de teclas (ex.: `Ctrl+Alt+F1`, `Ctrl+F2`) para qualquer janela aberta, a cada X segundos.

## Usar

Abra `target\release\keypulse.exe` — sem instalação nem login.

1. Escolha a janela na lista à esquerda.
2. Monte a sequência de combinações (grave pelo teclado ou escolha na paleta).
3. Defina o intervalo, a pausa entre combinações e quantas repetições.
4. Clique em **Iniciar** (ou `Ctrl+Shift+F12` de qualquer lugar).

A configuração é salva automaticamente em `%APPDATA%\KeyPulse\config.json`.

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
