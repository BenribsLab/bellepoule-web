//! Points d'une compétition (préférences, partie « Points ») : selon la
//! place au classement général, pour un classement suivi sur la saison.
//!
//! Par défaut, sur N participants, le 1er gagne N points et le dernier 1.
//! S'y ajoutent des bonus par place (le 1er : +X…) et pour « finir dans les
//! X » (dans les 4, dans les 8…), et un poids de la compétition. La formule
//! est modifiable ; variables (notation de la fédération) : N (participants),
//! R (rang), C (coefficient, le poids) et B (bonus).

use serde::{Deserialize, Serialize};

pub const DEFAULT_FORMULA: &str = "(N - R + 1 + B) * C";

/// Bonus d'une place précise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaceBonus {
    pub place: u32,
    pub points: f64,
}

/// Bonus pour finir dans les `top` premiers (dans les 4, dans les 8…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TopBonus {
    pub top: u32,
    pub points: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PointsSettings {
    #[serde(default)]
    pub enabled: bool,
    /// Poids de la compétition.
    #[serde(default = "one")]
    pub weight: f64,
    #[serde(default)]
    pub place_bonus: Vec<PlaceBonus>,
    #[serde(default)]
    pub top_bonus: Vec<TopBonus>,
    #[serde(default = "default_formula")]
    pub formula: String,
}

fn one() -> f64 {
    1.0
}

fn default_formula() -> String {
    DEFAULT_FORMULA.into()
}

impl Default for PointsSettings {
    fn default() -> Self {
        PointsSettings { enabled: false, weight: 1.0, place_bonus: Vec::new(), top_bonus: Vec::new(), formula: default_formula() }
    }
}

/// Ancienne écriture du « Classement simple » (avant la notation N, R, C, B).
const OLD_DEFAULT_FORMULA: &str = "(participants - place + 1 + bonus) * poids";

impl PointsSettings {
    /// Réglages relus d'un fichier : l'ancienne écriture du « Classement
    /// simple » passe à la notation de la fédération.
    pub fn upgraded(mut self) -> Self {
        if self.formula.trim() == OLD_DEFAULT_FORMULA {
            self.formula = DEFAULT_FORMULA.into();
        }
        self
    }

    /// Bonus d'une place : bonus de cette place, plus les « dans les X ».
    pub fn bonus(&self, place: u32) -> f64 {
        self.place_bonus.iter().filter(|b| b.place == place).map(|b| b.points).sum::<f64>()
            + self.top_bonus.iter().filter(|b| place <= b.top).map(|b| b.points).sum::<f64>()
    }

    /// Points d'une place sur `participants` (arrondis au centième ; jamais
    /// négatifs). `None` : formule invalide.
    pub fn points(&self, participants: u32, place: u32) -> Option<f64> {
        let value = evaluate(&self.formula, &|name| match name {
            "participants" | "n" => Some(participants as f64),
            "place" => Some(place as f64),
            "bonus" => Some(self.bonus(place)),
            "poids" | "weight" => Some(self.weight),
            _ => None,
        })?;
        Some(((value * 100.0).round() / 100.0).max(0.0))
    }

    /// Message d'erreur de la formule, s'il y en a une.
    pub fn formula_error(&self) -> Option<String> {
        self.points(10, 1).is_none().then(|| format!("formule invalide : « {} »", self.formula))
    }
}

/// Points sous forme lisible (« 12 », « 7,5 »).
pub fn image(points: f64) -> String {
    if (points - points.round()).abs() < 1e-9 {
        format!("{}", points.round() as i64)
    } else {
        format!("{points:.2}").trim_end_matches('0').replace('.', ",")
    }
}

// ------------------------------------------------- calcul de la formule

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Op(char),
}

fn tokens(text: &str) -> Option<Vec<Token>> {
    // « P = … » : seule la partie à droite du signe égal compte.
    let text = text.rsplit_once('=').map(|(_, right)| right).unwrap_or(text);
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit())) {
            // Nombre : virgule ou point décimal (« 1,01 »). Une virgule suivie
            // d'autre chose qu'un chiffre sépare deux arguments.
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit() || ((chars[i] == '.' || chars[i] == ',') && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit())))
            {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect::<String>().replace(',', ".");
            out.push(Token::Number(s.parse().ok()?));
        } else if c.is_alphabetic() || c == '_' || c == '<' {
            // « <nombre de participant> » est accepté comme « participants » ;
            // « ARRONDI.SUP » est un seul nom.
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric()
                    || chars[i] == '_'
                    || chars[i] == '<'
                    || chars[i] == '>'
                    || (chars[i] == '.' && chars.get(i + 1).is_some_and(|n| n.is_alphabetic()))
                    || (chars[i] == ' ' && chars[start] == '<' && !chars[start..i].contains(&'>')))
            {
                i += 1;
            }
            let raw: String = chars[start..i].iter().collect();
            // Plusieurs mots à la suite : un seul nom (« Poids de la compétition »).
            match out.last_mut() {
                Some(Token::Name(previous)) if !is_function(previous) => {
                    let joined = format!("{previous} {raw}");
                    *previous = normalize_name(&joined);
                }
                _ => out.push(Token::Name(normalize_name(&raw))),
            }
        } else if c == ',' || c == ';' {
            out.push(Token::Op(';'));
            i += 1;
        } else if "+-*/()^×÷".contains(c) {
            out.push(Token::Op(match c {
                '×' => '*',
                '÷' => '/',
                c => c,
            }));
            i += 1;
        } else {
            return None;
        }
    }
    Some(out)
}

/// Fonctions connues (noms français d'Excel et noms anglais).
const FUNCTIONS: [&str; 22] = [
    "log", "ln", "exp", "abs", "racine", "sqrt", "puissance", "power", "min", "max", "arrondi", "round", "arrondi.sup", "roundup",
    "arrondi.inf", "rounddown", "plafond", "ceiling", "plancher", "floor", "ent", "int",
];

fn is_function(name: &str) -> bool {
    FUNCTIONS.contains(&name)
}

/// Noms : sans accents ni majuscules. Notation de la fédération : N
/// (participants), R (rang), C (coefficient), B (bonus) ; les noms longs
/// (participants, place, poids, bonus) sont aussi acceptés.
fn normalize_name(raw: &str) -> String {
    let s: String = raw
        .trim_matches(|c| c == '<' || c == '>')
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            c => c,
        })
        .collect();
    if is_function(&s) {
        return s;
    }
    // « bonus par place » est un bonus : il passe avant « place ».
    if s == "b" || s.contains("bonus") {
        "bonus".into()
    } else if s == "c" || s.contains("poids") || s.contains("weight") || s.contains("coef") {
        "poids".into()
    } else if s == "n" || s.contains("participant") {
        "participants".into()
    } else if s == "r" || s.contains("place") || s.contains("rang") {
        "place".into()
    } else {
        s
    }
}

/// Arrondi à `digits` chiffres après la virgule avec la fonction donnée.
fn round_with(x: f64, digits: f64, f: fn(f64) -> f64) -> f64 {
    let factor = 10f64.powi(digits as i32);
    f(x * factor) / factor
}

/// Arrondi « à l'écart de zéro » (ARRONDI.SUP d'Excel).
fn away_from_zero(x: f64) -> f64 {
    if x >= 0.0 { x.ceil() } else { x.floor() }
}

/// Arrondi « vers zéro » (ARRONDI.INF d'Excel).
fn toward_zero(x: f64) -> f64 {
    x.trunc()
}

fn call(name: &str, args: &[f64]) -> Option<f64> {
    let a = |i: usize| args.get(i).copied();
    let digits = a(1).unwrap_or(0.0);
    Some(match name {
        "log" => match a(1) {
            Some(base) => a(0)?.ln() / base.ln(),
            None => a(0)?.log10(),
        },
        "ln" => a(0)?.ln(),
        "exp" => a(0)?.exp(),
        "abs" => a(0)?.abs(),
        "racine" | "sqrt" => a(0)?.sqrt(),
        "puissance" | "power" => a(0)?.powf(a(1)?),
        "min" => args.iter().copied().reduce(f64::min)?,
        "max" => args.iter().copied().reduce(f64::max)?,
        "arrondi" | "round" => round_with(a(0)?, digits, f64::round),
        "arrondi.sup" | "roundup" => round_with(a(0)?, digits, away_from_zero),
        "arrondi.inf" | "rounddown" => round_with(a(0)?, digits, toward_zero),
        "plafond" | "ceiling" => a(0)?.ceil(),
        "plancher" | "floor" | "ent" | "int" => a(0)?.floor(),
        _ => return None,
    })
}

/// Calcule une expression : nombres, variables, + - * / ^, parenthèses et
/// fonctions (LOG, LN, ARRONDI.SUP…, arguments séparés par « ; »).
pub fn evaluate(text: &str, var: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    let tokens = tokens(text)?;
    let mut parser = Parser { tokens: &tokens, at: 0, var };
    let value = parser.sum()?;
    (parser.at == tokens.len() && value.is_finite()).then_some(value)
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    var: &'a dyn Fn(&str) -> Option<f64>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn expect(&mut self, op: char) -> Option<()> {
        (self.peek() == Some(&Token::Op(op))).then(|| self.at += 1)
    }

    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        while let Some(Token::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.at += 1;
            let rhs = self.product()?;
            value = if op == '+' { value + rhs } else { value - rhs };
        }
        Some(value)
    }

    fn product(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        while let Some(Token::Op(op @ ('*' | '/'))) = self.peek().cloned() {
            self.at += 1;
            let rhs = self.unary()?;
            value = if op == '*' { value * rhs } else { value / rhs };
        }
        Some(value)
    }

    fn unary(&mut self) -> Option<f64> {
        if let Some(Token::Op('-')) = self.peek() {
            self.at += 1;
            return Some(-self.unary()?);
        }
        self.power()
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.atom()?;
        if let Some(Token::Op('^')) = self.peek() {
            self.at += 1;
            let exponent = self.unary()?;
            return Some(base.powf(exponent));
        }
        Some(base)
    }

    fn atom(&mut self) -> Option<f64> {
        let token = self.peek()?.clone();
        self.at += 1;
        match token {
            Token::Number(n) => Some(n),
            Token::Name(name) if self.peek() == Some(&Token::Op('(')) => {
                self.at += 1;
                let mut args = Vec::new();
                if self.peek() != Some(&Token::Op(')')) {
                    args.push(self.sum()?);
                    while self.peek() == Some(&Token::Op(';')) {
                        self.at += 1;
                        args.push(self.sum()?);
                    }
                }
                self.expect(')')?;
                call(&name, &args)
            }
            Token::Name(name) => (self.var)(&name),
            Token::Op('(') => {
                let value = self.sum()?;
                self.expect(')')?;
                Some(value)
            }
            Token::Op(_) => None,
        }
    }
}

// --------------------------------------------------- formules enregistrées

/// Une formule nommée.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedFormula {
    pub name: String,
    pub formula: String,
}

/// Formules fournies (non modifiables, toujours proposées).
pub fn builtin_formulas() -> Vec<NamedFormula> {
    vec![
        NamedFormula { name: "Classement simple".into(), formula: DEFAULT_FORMULA.into() },
        NamedFormula { name: "Classement officiel M15".into(), formula: "(1,01 - LOG(R) / LOG(N)) * C".into() },
        NamedFormula {
            name: "Classement officiel".into(),
            formula: "ARRONDI.SUP((1 - LOG(R) / LOG(512)) * C * (100 - 3 * ARRONDI.SUP(LN(R) / LN(2); 0)); 0)".into(),
        },
    ]
}

/// Fichier des formules enregistrées par l'utilisateur.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaStore {
    #[serde(default)]
    pub formulas: Vec<NamedFormula>,
}

impl FormulaStore {
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }

    /// Formules fournies puis enregistrées.
    pub fn all(&self) -> Vec<NamedFormula> {
        let mut list = builtin_formulas();
        list.extend(self.formulas.iter().cloned());
        list
    }

    /// Enregistre (ou remplace) une formule. Le nom d'une formule fournie
    /// est refusé.
    pub fn put(&mut self, name: &str, formula: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("donnez un nom à la formule".into());
        }
        if builtin_formulas().iter().any(|f| f.name == name) {
            return Err("cette formule est fournie : enregistrez-la sous un autre nom".into());
        }
        if evaluate(formula, &|_| Some(2.0)).is_none() {
            return Err(format!("formule invalide : « {formula} »"));
        }
        match self.formulas.iter_mut().find(|f| f.name == name) {
            Some(f) => f.formula = formula.trim().to_string(),
            None => self.formulas.push(NamedFormula { name: name.into(), formula: formula.trim().into() }),
        }
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.formulas.len();
        self.formulas.retain(|f| f.name != name);
        self.formulas.len() != before
    }
}

/// Explication pour les utilisateurs (préférences « Points »).
pub const HELP: &str = "Une formule calcule les points P d'un tireur d'après son rang final.

Variables (notation de la fédération) :
  N : nombre de tireurs classés
  R : rang final du tireur (1 pour le premier)
  C : coefficient de la compétition (champ « Poids de la compétition (C) »)
  B : bonus du tireur (bonus de sa place et bonus « dans les X »)

Opérations : + - * / ^ (puissance) et parenthèses. Nombres avec une virgule ou un point (1,01).
Fonctions (arguments séparés par « ; ») :
  LOG(x) logarithme décimal · LN(x) logarithme népérien
  ARRONDI(x; n) · ARRONDI.SUP(x; n) arrondi supérieur · ARRONDI.INF(x; n) arrondi inférieur (n chiffres après la virgule)
  MIN(a; b) · MAX(a; b) · ABS(x) · RACINE(x) · PUISSANCE(x; y)

Formules fournies :
  Classement simple : (N - R + 1 + B) * C ; sur 10 tireurs avec C = 1, 10 points au premier et 1 au dernier
  Classement officiel M15 : (1,01 - LOG(R) / LOG(N)) * C
  Classement officiel : ARRONDI.SUP((1 - LOG(R) / LOG(512)) * C * (100 - 3 * ARRONDI.SUP(LN(R) / LN(2); 0)); 0)

Pour garder une formule, donnez-lui un nom et enregistrez-la : elle sera proposée pour toutes vos compétitions.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_gives_n_to_the_first_and_1_to_the_last() {
        let s = PointsSettings { enabled: true, ..Default::default() };
        assert_eq!(s.points(10, 1), Some(10.0));
        assert_eq!(s.points(10, 10), Some(1.0));
    }

    #[test]
    fn bonuses_and_weight() {
        let s = PointsSettings {
            enabled: true,
            weight: 5.0,
            place_bonus: vec![PlaceBonus { place: 1, points: 3.0 }],
            top_bonus: vec![TopBonus { top: 4, points: 2.0 }, TopBonus { top: 8, points: 1.0 }],
            ..Default::default()
        };
        // (10 - 1 + 1 + 3 + 2 + 1) × 5
        assert_eq!(s.points(10, 1), Some(80.0));
        // (10 - 6 + 1 + 1) × 5
        assert_eq!(s.points(10, 6), Some(30.0));
        assert_eq!(s.points(10, 9), Some(10.0));
    }

    #[test]
    fn free_formula_with_the_user_notation() {
        let s = PointsSettings {
            formula: "(<nombre de participant> - <place du joueur> + <bonus par place>) * Poids de la compete".into(),
            weight: 2.0,
            ..Default::default()
        };
        assert_eq!(s.points(10, 1), Some(18.0));
        assert!(PointsSettings { formula: "place +".into(), ..Default::default() }.formula_error().is_some());
        assert_eq!(evaluate("2 * (3 + 4) / 2 - -1", &|_| None), Some(8.0));
        assert_eq!(image(7.5), "7,5");
        assert_eq!(image(12.0), "12");
    }

    #[test]
    fn ffe_formulas() {
        let m15 = PointsSettings { formula: "P = (1,01 - LOG(R) / LOG(N)) * C".into(), weight: 100.0, ..Default::default() };
        assert_eq!(m15.points(10, 1), Some(101.0));
        assert_eq!(m15.points(10, 10), Some(1.0));
        let others = PointsSettings { formula: builtin_formulas()[2].formula.clone(), weight: 1.0, ..Default::default() };
        // R = 1 : 100 ; R = 2 : ⌈0,8889 × 97⌉ = 87 ; R = 3 : ⌈0,8239 × 94⌉ = 78.
        assert_eq!(others.points(64, 1), Some(100.0));
        assert_eq!(others.points(64, 2), Some(87.0));
        assert_eq!(others.points(64, 3), Some(78.0));
        assert_eq!(evaluate("ARRONDI.SUP(2,31; 1) + MAX(1; 4; 2) + 2^3", &|_| None), Some(14.4));
        assert_eq!(evaluate("ROUND(2.5, 0)", &|_| None), Some(3.0), "virgule entre arguments");
    }

    #[test]
    fn formula_library() {
        let mut store = FormulaStore::default();
        assert!(store.put("Classement officiel M15", "R").is_err(), "nom d'une formule fournie");
        assert!(store.put("Club", "place +").is_err());
        store.put("Club", "100 / R").unwrap();
        store.put("Club", "50 / R").unwrap();
        assert_eq!(store.all().len(), 4);
        assert_eq!(store.formulas[0].formula, "50 / R");
        assert!(store.remove("Club"));
    }
}

#[cfg(test)]
mod upgrade_tests {
    #[test]
    fn old_simple_formula_uses_the_federation_notation() {
        let s = super::PointsSettings { formula: "(participants - place + 1 + bonus) * poids".into(), ..Default::default() }.upgraded();
        assert_eq!(s.formula, super::DEFAULT_FORMULA);
    }
}
