//! GDesk Remoto (GD Soluções): licença conferida NO SERVIDOR.
//!
//! O app manda o token de login do técnico (conta do GDesk) em
//! PunchHoleRequest/RequestRelay. Aqui o hbbs pergunta ao gdesk-backend se o
//! token tem licença do GDesk Remoto (POST GDESK_API_VALIDAR, cache de 60 s):
//!   - com licença: fluxo normal do RustDesk (conexão direta quando dá);
//!   - sem licença / sem login: força a sessão pelo relay LIMITADO
//!     (GDESK_RELAY_LIMITADO -- um segundo hbbr com GDESK_LIMITE_SESSAO=180),
//!     que derruba a sessão aos 3 min. O endereço da outra máquina nunca é
//!     entregue, então não tem como forçar conexão direta.
//! Sem GDESK_API_VALIDAR e GDESK_RELAY_LIMITADO, nada muda (comportamento
//! original). GDESK_API_SEGREDO = mesmo valor de REMOTO_VALIDAR_SEGREDO no backend.
use hbb_common::{log, tokio::sync::Mutex};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const VALIDADE_CACHE: Duration = Duration::from_secs(60);
const MAX_CACHE: usize = 20_000;

lazy_static::lazy_static! {
    static ref CACHE: Mutex<HashMap<String, (bool, Instant)>> = Default::default();
}

fn url_validar() -> String {
    crate::common::get_arg("GDESK_API_VALIDAR")
}

pub fn relay_limitado() -> String {
    crate::common::get_arg("GDESK_RELAY_LIMITADO")
}

pub fn ativo() -> bool {
    !url_validar().is_empty() && !relay_limitado().is_empty()
}

pub async fn licenciado(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if let Some((valor, quando)) = CACHE.lock().await.get(token) {
        if quando.elapsed() < VALIDADE_CACHE {
            return *valor;
        }
    }
    match consultar(token).await {
        Ok(valor) => {
            let mut cache = CACHE.lock().await;
            if cache.len() > MAX_CACHE {
                cache.clear();
            }
            cache.insert(token.to_owned(), (valor, Instant::now()));
            valor
        }
        Err(err) => {
            // GDesk fora do ar: usa a última resposta conhecida; sem nenhuma,
            // trata como sem licença (sessão limitada, nunca bloqueada).
            log::warn!("GDesk: falha ao validar licença: {}", err);
            CACHE.lock().await.get(token).map(|x| x.0).unwrap_or(false)
        }
    }
}

async fn consultar(token: &str) -> Result<bool, reqwest::Error> {
    let cliente = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let resposta: serde_json::Value = cliente
        .post(url_validar())
        .header("X-GDesk-Segredo", crate::common::get_arg("GDESK_API_SEGREDO"))
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await?
        .json()
        .await?;
    Ok(resposta
        .get("licenca")
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}
