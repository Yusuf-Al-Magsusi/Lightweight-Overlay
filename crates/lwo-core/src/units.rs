//! Display units.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TemperatureUnit {
    pub const ALL: [TemperatureUnit; 2] = [TemperatureUnit::Celsius, TemperatureUnit::Fahrenheit];

    pub fn label(self) -> &'static str {
        match self {
            TemperatureUnit::Celsius => "Celsius (\u{00B0}C)",
            TemperatureUnit::Fahrenheit => "Fahrenheit (\u{00B0}F)",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            TemperatureUnit::Celsius => "\u{00B0}C",
            TemperatureUnit::Fahrenheit => "\u{00B0}F",
        }
    }

    /// A Celsius reading as text in this unit, for example "62 °C" or "144 °F".
    pub fn format(self, celsius: u32) -> String {
        match self {
            TemperatureUnit::Celsius => format!("{celsius} \u{00B0}C"),
            TemperatureUnit::Fahrenheit => {
                let f = (f64::from(celsius) * 9.0 / 5.0 + 32.0).round() as i64;
                format!("{f} \u{00B0}F")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_celsius() {
        assert_eq!(TemperatureUnit::Celsius.format(62), "62 \u{00B0}C");
    }

    #[test]
    fn converts_to_fahrenheit_with_rounding() {
        let f = TemperatureUnit::Fahrenheit;
        assert_eq!(f.format(0), "32 \u{00B0}F");
        assert_eq!(f.format(100), "212 \u{00B0}F");
        assert_eq!(f.format(62), "144 \u{00B0}F"); // 143.6 rounds to 144
        assert_eq!(f.format(37), "99 \u{00B0}F"); // 98.6 rounds to 99
    }

    #[test]
    fn default_is_celsius() {
        assert_eq!(TemperatureUnit::default(), TemperatureUnit::Celsius);
    }
}
