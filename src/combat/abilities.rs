#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityClass {
    CaptainManeuver,
    BridgeAbility,
    BelowDeck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingWindow {
    CombatBegin,
    RoundStart,
    AttackPhase,
    DefensePhase,
    RoundEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrewSeat {
    Captain,
    Bridge,
    BelowDeck,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AbilityEffect {
    AttackMultiplier(f64),
    PierceBonus(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ability {
    pub name: String,
    pub class: AbilityClass,
    pub timing: TimingWindow,
    pub boostable: bool,
    pub effect: AbilityEffect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrewSeatContext {
    pub seat: CrewSeat,
    pub ability: Ability,
    pub boosted: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CrewConfiguration {
    pub seats: Vec<CrewSeatContext>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAbilityEffect {
    pub ability_name: String,
    pub effect: AbilityEffect,
    pub boosted: bool,
}

impl AbilityClass {
    pub const fn allowed_seat(self) -> CrewSeat {
        match self {
            Self::CaptainManeuver => CrewSeat::Captain,
            Self::BridgeAbility => CrewSeat::Bridge,
            Self::BelowDeck => CrewSeat::BelowDeck,
        }
    }
}

pub fn can_activate_in_seat(context: &CrewSeatContext) -> bool {
    context.seat == context.ability.class.allowed_seat()
        && (context.ability.boostable || !context.boosted)
}

pub fn active_effects_for_timing(
    crew: &CrewConfiguration,
    timing: TimingWindow,
) -> Vec<ActiveAbilityEffect> {
    crew.seats
        .iter()
        .filter(|seat_context| {
            can_activate_in_seat(seat_context) && seat_context.ability.timing == timing
        })
        .map(|seat_context| ActiveAbilityEffect {
            ability_name: seat_context.ability.name.clone(),
            effect: seat_context.ability.effect,
            boosted: seat_context.boosted,
        })
        .collect()
}
