use crate::{Error, FixedPrice};

impl FixedPrice {
    pub(crate) fn amount_out(self, collateral: u128) -> Result<u128, Error> {
        mul_div(collateral, self.token_units, self.collateral_units, false)
    }

    pub(crate) fn amount_in(self, tokens: u128) -> Result<u128, Error> {
        mul_div(tokens, self.collateral_units, self.token_units, true)
    }
}

fn mul_div(value: u128, multiplier: u128, divisor: u128, ceil: bool) -> Result<u128, Error> {
    if divisor == 0 {
        return Err(Error::InvalidTerms);
    }
    let whole = (value / divisor)
        .checked_mul(multiplier)
        .ok_or(Error::Arithmetic)?;
    let numerator = (value % divisor)
        .checked_mul(multiplier)
        .ok_or(Error::Arithmetic)?;
    let floor = whole
        .checked_add(numerator / divisor)
        .ok_or(Error::Arithmetic)?;
    floor
        .checked_add(u128::from(ceil && numerator % divisor != 0))
        .ok_or(Error::Arithmetic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_price_rounds_output_down_and_required_input_up() {
        let price = FixedPrice {
            collateral_units: 3,
            token_units: 2,
        };
        assert_eq!(price.amount_out(4), Ok(2));
        assert_eq!(price.amount_in(3), Ok(5));
        assert_eq!(price.amount_out(0), Ok(0));
        assert_eq!(price.amount_in(0), Ok(0));
    }

    #[test]
    fn fixed_price_preserves_checked_full_width_arithmetic() {
        let price = FixedPrice {
            collateral_units: 1,
            token_units: 1,
        };
        assert_eq!(price.amount_out(u128::MAX), Ok(u128::MAX));
        assert_eq!(price.amount_in(u128::MAX), Ok(u128::MAX));
        let price = FixedPrice {
            collateral_units: 2,
            token_units: 1,
        };
        assert_eq!(price.amount_in(u128::MAX), Err(Error::Arithmetic));
        let price = FixedPrice {
            collateral_units: 0,
            token_units: 0,
        };
        assert_eq!(price.amount_out(1), Err(Error::InvalidTerms));
        assert_eq!(price.amount_in(1), Err(Error::InvalidTerms));
    }
}
