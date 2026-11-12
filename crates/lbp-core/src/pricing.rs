use sale_policy::arithmetic::{ArithmeticError, Rounding, mul_div};

use crate::{Error, FixedPrice};

impl FixedPrice {
    pub(crate) fn amount_out(self, collateral: u128) -> Result<u128, Error> {
        mul_div(
            collateral,
            self.token_units,
            self.collateral_units,
            Rounding::Floor,
        )
        .map_err(pricing_error)
    }

    pub(crate) fn amount_in(self, tokens: u128) -> Result<u128, Error> {
        mul_div(
            tokens,
            self.collateral_units,
            self.token_units,
            Rounding::Ceil,
        )
        .map_err(pricing_error)
    }
}

fn pricing_error(error: ArithmeticError) -> Error {
    match error {
        ArithmeticError::DivisionByZero => Error::InvalidTerms,
        ArithmeticError::Overflow => Error::Arithmetic,
    }
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
            collateral_units: 1,
            token_units: 2,
        };
        assert_eq!(price.amount_out(u128::MAX), Err(Error::Arithmetic));
        let price = FixedPrice {
            collateral_units: u128::MAX,
            token_units: u128::MAX,
        };
        assert_eq!(price.amount_out(u128::MAX - 1), Err(Error::Arithmetic));
        assert_eq!(price.amount_in(u128::MAX - 1), Err(Error::Arithmetic));
        let price = FixedPrice {
            collateral_units: 0,
            token_units: 0,
        };
        assert_eq!(price.amount_out(1), Err(Error::InvalidTerms));
        assert_eq!(price.amount_in(1), Err(Error::InvalidTerms));
        assert_eq!(price.amount_out(0), Err(Error::InvalidTerms));
        assert_eq!(price.amount_in(0), Err(Error::InvalidTerms));
    }
}
