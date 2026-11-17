//! Leitor mínimo de `.env` (ao lado do .exe). Formato: `CHAVE=valor`, `#` comenta,
//! aspas opcionais. Variável já definida no ambiente tem prioridade sobre o arquivo.

pub fn parse(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim();
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let mut v = v.trim();
        if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
            v = &v[1..v.len() - 1];
        } else if let Some(i) = v.find(" #") {
            v = v[..i].trim_end();
        }
        out.push((k.to_string(), v.to_string()));
    }
    out
}

/// "1", "true", "on", "sim", "ligado" = true; "0", "false", "off", "nao", "não", "desligado" = false.
pub fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_lowercase().as_str() {
        "1" | "true" | "on" | "sim" | "s" | "yes" | "ligado" => Some(true),
        "0" | "false" | "off" | "nao" | "não" | "n" | "no" | "desligado" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato() {
        let v = parse("# comentário\nZEN_AUTO_TOKEN=1\n export A = \"x y\" \nB='z'\nC=valor # nota\nruim\n1-X=2\n=3\n");
        assert_eq!(
            v,
            vec![
                ("ZEN_AUTO_TOKEN".into(), "1".into()),
                ("A".into(), "x y".into()),
                ("B".into(), "z".into()),
                ("C".into(), "valor".into()),
            ]
        );
    }

    #[test]
    fn booleanos() {
        assert_eq!(parse_bool(" ON "), Some(true));
        assert_eq!(parse_bool("não"), Some(false));
        assert_eq!(parse_bool("talvez"), None);
    }
}
