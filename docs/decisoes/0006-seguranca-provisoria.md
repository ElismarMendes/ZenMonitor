# 0006 — Segurança provisória até a fase 4

Hoje: WebSocket sem TLS, painel sem login (só leitura), comandos protegidos por `admin_token` do
`central.toml`, tokens por PC escritos à mão. Comparação de token em tempo constante; comandos são uma
lista fechada no protocolo (não existe variante com shell ou caminho de arquivo); cada comando vai para a
tabela `audit`.

**Isso só é aceitável na rede de casa.** Não abra a porta 8787 no roteador. A fase 4 traz TLS com
certificado fixado no pareamento, login com Argon2 e pareamento por código de 6 dígitos.
