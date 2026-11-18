//! Avisos fora do PC. Falha de envio é registrada e ignorada: nunca derruba o central.

use crate::config::CentralConfig;

pub async fn send(client: &reqwest::Client, cfg: &CentralConfig, text: &str) {
    if !cfg.ntfy_url.is_empty() {
        let r = client
            .post(&cfg.ntfy_url)
            .header("Title", "ZenMonitor")
            .header("Priority", "high")
            .body(text.to_string())
            .send()
            .await;
        match r {
            Err(e) => tracing::warn!("ntfy falhou: {e}"),
            // 18/11 19:22: o ntfy.sh grátis tem limite diário (~250 mensagens) e responde 429;
            // antes isso não era registrado e as notificações sumiam caladas.
            Ok(resp) if !resp.status().is_success() => {
                let espera = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).unwrap_or("?").to_string();
                tracing::warn!(status = resp.status().as_u16(), retry_after = %espera, "ntfy recusou a notificação (limite do ntfy.sh?)");
            }
            Ok(_) => tracing::info!("ntfy: notificação enviada"),
        }
    }
    if !cfg.telegram_bot_token.is_empty() && !cfg.telegram_chat_id.is_empty() {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", cfg.telegram_bot_token);
        let r = client
            .post(url)
            .form(&[("chat_id", cfg.telegram_chat_id.as_str()), ("text", text)])
            .send()
            .await;
        if let Err(e) = r {
            tracing::warn!("telegram falhou: {e}");
        }
    }
}

pub async fn heartbeat(client: &reqwest::Client, url: &str) {
    if !url.is_empty() {
        let _ = client.get(url).send().await;
    }
}
