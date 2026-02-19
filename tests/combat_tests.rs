use kobayashi::combat::{
    component_mitigation, mitigation, AttackerStats, DefenderStats, ShipType, EPSILON,
};

fn approx_eq(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "expected {b}, got {a}");
}

#[test]
fn component_mitigation_clamps_non_positive_piercing_to_epsilon() {
    let with_zero = component_mitigation(10.0, 0.0);
    let with_negative = component_mitigation(10.0, -5.0);
    let with_epsilon = component_mitigation(10.0, EPSILON);

    approx_eq(with_zero, with_epsilon, 1e-15);
    approx_eq(with_negative, with_epsilon, 1e-15);
}

#[test]
fn component_mitigation_clamps_negative_defense_to_zero() {
    let clamped = component_mitigation(-10.0, 5.0);
    let zero = component_mitigation(0.0, 5.0);

    approx_eq(clamped, zero, 1e-15);
}

#[test]
fn mitigation_output_is_bounded_for_extreme_inputs() {
    let low = mitigation(
        DefenderStats {
            armor: -1.0,
            shield_deflection: -5.0,
            dodge: -10.0,
        },
        AttackerStats {
            armor_piercing: 1e12,
            shield_piercing: 1e12,
            accuracy: 1e12,
        },
        ShipType::Survey,
    );
    let high = mitigation(
        DefenderStats {
            armor: 1e12,
            shield_deflection: 1e12,
            dodge: 1e12,
        },
        AttackerStats {
            armor_piercing: 0.0,
            shield_piercing: -1.0,
            accuracy: EPSILON / 2.0,
        },
        ShipType::Interceptor,
    );

    assert!(
        (0.0..=1.0).contains(&low),
        "low mitigation out of bounds: {low}"
    );
    assert!(
        (0.0..=1.0).contains(&high),
        "high mitigation out of bounds: {high}"
    );
}

#[test]
fn golden_values_match_python_reference_for_each_ship_type() {
    let defender = DefenderStats {
        armor: 100.0,
        shield_deflection: 80.0,
        dodge: 60.0,
    };
    let attacker = AttackerStats {
        armor_piercing: 50.0,
        shield_piercing: 40.0,
        accuracy: 30.0,
    };

    approx_eq(
        mitigation(defender, attacker, ShipType::Survey),
        0.5489034243492552,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Battleship),
        0.5914393181871193,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Explorer),
        0.5914393181871193,
        1e-12,
    );
    approx_eq(
        mitigation(defender, attacker, ShipType::Interceptor),
        0.5914393181871193,
        1e-12,
    );
}
