//! Formatação pt-BR usada no painel e nas notificações.

/// 423485898 -> "423.485.898"
pub fn int_br(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

/// Zen por hora em formato curto: 18_500_000 -> "+18,5M/h"; 1_250_000_000 -> "+1,25B/h".
pub fn rate_br(per_hour: f64) -> String {
    if !per_hour.is_finite() || per_hour <= 0.0 {
        return "+0/h".to_string();
    }
    let (v, suf, dec) = if per_hour >= 1e9 {
        (per_hour / 1e9, "B", 2)
    } else if per_hour >= 1e6 {
        (per_hour / 1e6, "M", 1)
    } else if per_hour >= 1e3 {
        (per_hour / 1e3, "k", 1)
    } else {
        (per_hour, "", 0)
    };
    let txt = format!("{v:.dec$}").replace('.', ",");
    let txt = if txt.contains(',') {
        txt.trim_end_matches('0').trim_end_matches(',').to_string()
    } else {
        txt
    };
    format!("+{txt}{suf}/h")
}

/// Duração curta: 75 -> "1m15s", 3700 -> "1h01m".
pub fn dur_br(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inteiros() {
        assert_eq!(int_br(0), "0");
        assert_eq!(int_br(999), "999");
        assert_eq!(int_br(1000), "1.000");
        assert_eq!(int_br(423_485_898), "423.485.898");
        assert_eq!(int_br(2_000_000_000), "2.000.000.000");
    }

    #[test]
    fn taxas() {
        assert_eq!(rate_br(18_500_000.0), "+18,5M/h");
        assert_eq!(rate_br(100_000_000.0), "+100M/h");
        assert_eq!(rate_br(1_250_000_000.0), "+1,25B/h");
        assert_eq!(rate_br(0.0), "+0/h");
        assert_eq!(rate_br(-5.0), "+0/h");
        assert_eq!(rate_br(f64::NAN), "+0/h");
    }

    #[test]
    fn duracoes() {
        assert_eq!(dur_br(5), "5s");
        assert_eq!(dur_br(75), "1m15s");
        assert_eq!(dur_br(3700), "1h01m");
    }
}
