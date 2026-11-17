//! KS é somente observação de desempenho; este módulo não produz decisões de movimento.
use crate::rate::ZenTracker;

pub struct Analysis {
    clean: ZenTracker,
    contested: ZenTracker,
    clean_since: Option<u64>,
    contested_since: Option<u64>,
    baseline: Option<f64>,
}
impl Default for Analysis {
    fn default() -> Self {
        Self {
            clean: ZenTracker::new(300),
            contested: ZenTracker::new(120),
            clean_since: None,
            contested_since: None,
            baseline: None,
        }
    }
}
impl Analysis {
    pub fn observe(&mut self, now: u64, zen: Option<u64>, foreign: &[String]) -> Option<String> {
        if foreign.is_empty() {
            if self.contested_since.take().is_some() {
                self.clean = ZenTracker::new(300);
                self.clean_since = None;
            }
            if let Some(z) = zen.filter(|z| *z < crate::vault::DEPOSIT_AT) {
                self.clean_since.get_or_insert(now);
                self.clean.push(now, z);
                if now.saturating_sub(self.clean_since.unwrap()) >= 120 {
                    self.baseline = Some(self.clean.per_hour(now));
                }
            }
            return None;
        }
        let names = foreign.join(", ");
        let Some(z) = zen else {
            return Some(format!("KS possível ({names}): Zen não confiável; permaneço no spot"));
        };
        if z >= crate::vault::DEPOSIT_AT {
            return Some(format!(
                "KS possível ({names}): Zen no limite, rendimento inconclusivo; permaneço no spot"
            ));
        }
        if self.contested_since.is_none() {
            self.contested_since = Some(now);
            self.contested = ZenTracker::new(120);
        }
        self.contested.push(now, z);
        if now.saturating_sub(self.contested_since.unwrap()) < 120 {
            return Some(format!(
                "KS possível ({names}): medindo Zen/h por 120 s; permaneço no spot"
            ));
        }
        let rate = self.contested.per_hour(now);
        let Some(base) = self.baseline.filter(|r| *r > 0.0) else {
            return Some(format!(
                "KS possível ({names}): {:.0} Zen/h, sem amostra anterior comparável; permaneço no spot",
                rate
            ));
        };
        let loss = ((base - rate) / base * 100.0).max(0.0);
        Some(if loss >= 20.0 {
            format!("KS ({names}): queda de {loss:.0}% coincidindo com presença externa ({rate:.0} vs {base:.0} Zen/h); permaneço no spot")
        } else {
            format!("KS possível ({names}): sem queda relevante ({rate:.0} vs {base:.0} Zen/h); permaneço no spot")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compara_janelas_e_nunca_determina_fuga() {
        let mut a = Analysis::default();
        for t in (0..=180).step_by(5) {
            a.observe(t, Some(100 + t * 100), &[]);
        }
        let names = vec!["Externo".into()];
        let mut status = None;
        for t in (185..=310).step_by(5) {
            status = a.observe(t, Some(18100 + (t - 180) * 10), &names);
        }
        let status = status.unwrap();
        assert!(status.contains("queda"));
        assert!(status.contains("permaneço no spot"));
    }
    #[test]
    fn sem_base_ou_zen_no_limite_nao_afirma_perda() {
        let mut a = Analysis::default();
        let names = vec!["Externo".into()];
        assert!(a
            .observe(0, Some(crate::vault::DEPOSIT_AT), &names)
            .unwrap()
            .contains("inconclusivo"));
        assert!(a.observe(1, None, &names).unwrap().contains("não confiável"));
    }
}
