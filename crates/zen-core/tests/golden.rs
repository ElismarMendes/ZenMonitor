//! Golden: cada linha de entrada -> uma linha de saída gravada em `.expected`.
//! Para regravar depois de uma mudança INTENCIONAL: `UPDATE_GOLDEN=1 cargo test -p zen-core`.
//! Revise o diff do `.expected` antes de commitar.

use std::path::Path;
use zen_core::chat::{parse_line, ChatConfig};

fn run(input: &Path) {
    let cfg = ChatConfig::default();
    let src = std::fs::read_to_string(input).unwrap();
    let mut out = String::new();
    for line in src.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        out.push_str(&format!("{line}\n  => {:?}\n", parse_line(line, &cfg)));
    }
    let exp = input.with_extension("expected");
    if std::env::var("UPDATE_GOLDEN").is_ok() || !exp.exists() {
        std::fs::write(&exp, &out).unwrap();
        return;
    }
    // O checkout no Windows pode trazer CRLF; a comparação é por conteúdo, não por fim de linha.
    let want = std::fs::read_to_string(&exp).unwrap().replace("\r\n", "\n");
    assert_eq!(out, want, "saída mudou para {}", input.display());
}

#[test]
fn chat_do_doc() {
    run(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chat_do_doc.txt"
    )));
}

/// Roda automaticamente quando os dados reais forem colocados no repositório.
#[test]
fn chat_real_se_existir() {
    let p = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/chat_real.txt"));
    if p.exists() {
        run(p);
    }
}
