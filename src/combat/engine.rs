/// Combat mitigation parity implementation migrated from
/// `tools/combat_engine/mitigation.py`.
///
/// The formulas and clamps in this module intentionally mirror the Python
/// reference behavior exactly.
#[derive(Debug, Clone, Copy)]
pub struct FightResult {
    pub won: bool,
}

pub const EPSILON: f64 = 1e-9;

pub const SURVEY_COEFFICIENTS: (f64, f64, f64) = (0.3, 0.3, 0.3);
pub const BATTLESHIP_COEFFICIENTS: (f64, f64, f64) = (0.55, 0.2, 0.2);
pub const EXPLORER_COEFFICIENTS: (f64, f64, f64) = (0.2, 0.55, 0.2);
pub const INTERCEPTOR_COEFFICIENTS: (f64, f64, f64) = (0.2, 0.2, 0.55);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipType {
    Survey,
    Battleship,
    Explorer,
    Interceptor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefenderStats {
    pub armor: f64,
    pub shield_deflection: f64,
    pub dodge: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackerStats {
    pub armor_piercing: f64,
    pub shield_piercing: f64,
    pub accuracy: f64,
}

impl ShipType {
    pub const fn coefficients(self) -> (f64, f64, f64) {
        match self {
            Self::Survey => SURVEY_COEFFICIENTS,
            Self::Battleship => BATTLESHIP_COEFFICIENTS,
            Self::Explorer => EXPLORER_COEFFICIENTS,
            Self::Interceptor => INTERCEPTOR_COEFFICIENTS,
        }
    }
}

/// Compute component mitigation f(x) = 1 / (1 + 4^(1.1 - x)).
pub fn component_mitigation(defense: f64, piercing: f64) -> f64 {
    let safe_defense = defense.max(0.0);
    let safe_piercing = piercing.max(EPSILON);
    let x = safe_defense / safe_piercing;
    1.0 / (1.0 + 4.0_f64.powf(1.1 - x))
}

/// Compute total mitigation using weighted multiplicative composition.
pub fn mitigation(defender: DefenderStats, attacker: AttackerStats, ship_type: ShipType) -> f64 {
    let (c_armor, c_shield, c_dodge) = ship_type.coefficients();

    let f_armor = component_mitigation(defender.armor, attacker.armor_piercing);
    let f_shield = component_mitigation(defender.shield_deflection, attacker.shield_piercing);
    let f_dodge = component_mitigation(defender.dodge, attacker.accuracy);

    let total =
        1.0 - (1.0 - c_armor * f_armor) * (1.0 - c_shield * f_shield) * (1.0 - c_dodge * f_dodge);
    total.clamp(0.0, 1.0)
}

pub fn simulate_once() -> FightResult {
    FightResult { won: true }
}
