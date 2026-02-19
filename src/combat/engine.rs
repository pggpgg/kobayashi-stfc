#[derive(Debug, Clone, Copy)]
pub struct FightResult {
    pub won: bool,
}

pub fn simulate_once() -> FightResult {
    FightResult { won: true }
}
