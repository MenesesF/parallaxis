//! # Parallaxis Normalizer
//!
//! Converts units to SI standard and resolves multilingual aliases.
//! "212°F" → 373.15K, "3200mg" → 3.2g, "paracetamol" → "acetaminophen"

use parallaxis_core::*;

/// Normalize a numeric value to the target unit.
pub fn normalize_number(value: f64, from: &Unit, to: &Unit) -> Result<f64> {
    match (from, to) {
        // Temperature
        (Unit::Fahrenheit, Unit::Kelvin) => Ok((value - 32.0) * 5.0 / 9.0 + 273.15),
        (Unit::Celsius, Unit::Kelvin) => Ok(value + 273.15),
        (Unit::Kelvin, Unit::Celsius) => Ok(value - 273.15),
        (Unit::Fahrenheit, Unit::Celsius) => Ok((value - 32.0) * 5.0 / 9.0),

        // Mass
        (Unit::Gram, Unit::Kilogram) => Ok(value / 1000.0),
        (Unit::Kilogram, Unit::Gram) => Ok(value * 1000.0),

        // Length
        (Unit::Kilometer, Unit::Meter) => Ok(value * 1000.0),
        (Unit::Meter, Unit::Kilometer) => Ok(value / 1000.0),

        // Area
        (Unit::SquareKilometer, Unit::SquareMeter) => Ok(value * 1_000_000.0),
        (Unit::SquareMeter, Unit::SquareKilometer) => Ok(value / 1_000_000.0),

        // Volume
        (Unit::Liter, Unit::CubicMeter) => Ok(value / 1000.0),
        (Unit::CubicMeter, Unit::Liter) => Ok(value * 1000.0),

        // Same unit
        (a, b) if a == b => Ok(value),

        _ => Err(ParallaxisError::Normalization(format!(
            "Cannot convert {:?} to {:?}",
            from, to
        ))),
    }
}

/// Compare two numeric values within a tolerance threshold.
pub fn values_match(claim_value: f64, vault_value: f64, tolerance: f64) -> ValueMatch {
    if claim_value == vault_value {
        return ValueMatch::Exact;
    }

    let deviation = if vault_value != 0.0 {
        ((claim_value - vault_value) / vault_value).abs()
    } else {
        f64::INFINITY
    };

    if deviation <= tolerance {
        ValueMatch::WithinTolerance { deviation }
    } else {
        ValueMatch::OutOfTolerance { deviation }
    }
}

#[derive(Debug)]
pub enum ValueMatch {
    Exact,
    WithinTolerance { deviation: f64 },
    OutOfTolerance { deviation: f64 },
}
