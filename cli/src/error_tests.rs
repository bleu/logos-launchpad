use super::{ErrorCategory, error_category};

#[test]
fn json_error_categories_are_stable() {
    for (category, expected) in [
        (ErrorCategory::SlippageFloor, "slippage_floor"),
        (
            ErrorCategory::SaleReserveOvershoot,
            "sale_reserve_overshoot",
        ),
        (
            ErrorCategory::CollateralReserveOvershoot,
            "collateral_reserve_overshoot",
        ),
        (ErrorCategory::General, "general"),
    ] {
        let label: &'static str = category.into();
        assert_eq!(label, expected);
        assert_eq!(category.to_string(), expected);
        assert_eq!(error_category(&eyre::Report::new(category)), category);
    }
}

#[test]
fn quote_error_variants_select_categories() {
    use launchpad_client::ClientError;
    use pool::SwapError;
    for (error, category) in [
        (
            ClientError::QuoteBuy(SwapError::SlippageExceeded),
            ErrorCategory::SlippageFloor,
        ),
        (
            ClientError::QuoteSell(SwapError::SlippageExceeded),
            ErrorCategory::SlippageFloor,
        ),
        (
            ClientError::QuoteBuyWithCollateral(SwapError::SlippageExceeded),
            ErrorCategory::SlippageFloor,
        ),
        (
            ClientError::QuoteBuy(SwapError::InsufficientRealOutputReserve),
            ErrorCategory::SaleReserveOvershoot,
        ),
        (
            ClientError::QuoteBuyWithCollateral(SwapError::InsufficientRealOutputReserve),
            ErrorCategory::SaleReserveOvershoot,
        ),
        (
            ClientError::QuoteSell(SwapError::InsufficientRealOutputReserve),
            ErrorCategory::CollateralReserveOvershoot,
        ),
        (
            ClientError::QuoteBuy(SwapError::Closed),
            ErrorCategory::General,
        ),
    ] {
        let report = eyre::Report::new(error).wrap_err("reading a quote");
        assert_eq!(error_category(&report), category);
    }
}

#[test]
fn error_text_cannot_select_a_category() {
    for message in [
        "slippage",
        "max_amount_in",
        "sale reserve",
        "real_reserve0",
        "collateral reserve",
        "real_reserve1",
    ] {
        assert_eq!(
            error_category(&eyre::eyre!(message)),
            ErrorCategory::General
        );
        let error = launchpad_client::ClientError::ReadFactoryPool(Box::new(
            std::io::Error::other(message),
        ));
        assert_eq!(
            error_category(&eyre::Report::new(error)),
            ErrorCategory::General
        );
    }
    let error = launchpad_client::ClientError::SubmitPrivateBuy(Box::new(
        wallet::ExecutionFailureKind::InsufficientFundsError,
    ));
    assert_eq!(
        error_category(&eyre::Report::new(error)),
        ErrorCategory::General
    );
}
