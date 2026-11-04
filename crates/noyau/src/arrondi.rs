//! Les arrondis de Python, à l'identique : le Monteur 1 relit ce que celui-ci écrit, et les
//! contrôles comparent les deux au milliardième.

/// `round(x, n)` de Python : la valeur binaire exacte arrondie à `n` décimales, au plus proche
/// (les égalités vers le chiffre pair), comme le fait aussi le formatage de Rust.
pub fn arrondi(x: f64, n: usize) -> f64 {
    format!("{x:.n$}").parse().unwrap_or(x)
}

/// `int(round(x))` de Python : au plus proche, les égalités vers l'entier pair.
pub fn entier(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// `len(texte)` de Python : des caractères, pas des octets.
pub fn longueur(texte: &str) -> usize {
    texte.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comme_python() {
        assert_eq!(arrondi(2.675, 2), 2.67); // 2.675 vaut 2.67499999… en binaire
        assert_eq!(arrondi(0.125, 2), 0.12);
        assert_eq!(arrondi(0.375, 2), 0.38);
        assert_eq!(arrondi(1.0005, 3), 1.0); // 1.000499999…
        assert_eq!(entier(2.5), 2);
        assert_eq!(entier(3.5), 4);
        assert_eq!(entier(-0.5), 0);
        assert_eq!(longueur("Égalité"), 7);
    }
}
